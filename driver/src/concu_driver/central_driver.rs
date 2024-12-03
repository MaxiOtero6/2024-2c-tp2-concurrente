use std::{collections::HashMap, sync::Arc};

use actix::{
    dev::ContextFutureSpawner, fut::wrap_future, Actor, Addr, AsyncContext, Context, Handler,
    Message, SpawnHandle,
};
use actix_async_handler::async_handler;
use common::utils::{
    json_parser::{PaymentMessages, TripMessages, TripStatus},
    position::Position,
};
use rayon::{
    iter::{IntoParallelIterator, ParallelIterator},
    slice::ParallelSliceMut,
};
use tokio::{sync::Mutex, time::sleep};

use crate::concu_driver::{
    consts::{MAX_DISTANCE, TAKE_TRIP_TIMEOUT_MS},
    driver_connection::{CheckACK, SendAll},
    handle_trip::{ClearPassenger, ForceNotifyPosition},
    json_parser::DriverMessages,
};

use super::{
    consts::ELECTION_TIMEOUT_DURATION, driver_connection::DriverConnection,
    handle_trip::TripHandler, passenger_connection::PassengerConnection,
    payment_connection::PaymentConnection,
};

pub struct CentralDriver {
    /// Direccion del actor TripHandler
    trip_handler: Addr<TripHandler>,
    /// Direccion del actor PassengerConnection
    passengers: Arc<Mutex<HashMap<u32, Addr<PassengerConnection>>>>,
    /// Direcciones de los drivers segun su id
    connection_with_drivers: HashMap<u32, Addr<DriverConnection>>, // 0...N
    /// Posiciones de los demas drivers segun su id,
    /// cobra sentido si este driver es lider
    driver_positions: HashMap<u32, Position>,
    /// Id del driver lider
    leader_id: Option<u32>,
    /// Id del driver
    id: u32,
    /// Timeout de la eleccion
    election_timeout: Option<SpawnHandle>,
}

impl Actor for CentralDriver {
    type Context = Context<Self>;
}

impl CentralDriver {
    /// Crea un nuevo actor `CentralDriver` con un id dado.
    pub fn create_new(id: u32) -> Addr<Self> {
        CentralDriver::create(|ctx| Self {
            id,
            leader_id: None,
            driver_positions: HashMap::new(),
            connection_with_drivers: HashMap::new(),
            trip_handler: TripHandler::new(ctx.address(), id).start(),
            passengers: Arc::new(Mutex::new(HashMap::new())),
            election_timeout: None,
        })
    }

    /// Verifica si el driver es el lider a partir de su id.
    fn im_leader(&self) -> bool {
        if let Some(lid) = self.leader_id {
            return self.id == lid;
        }

        false
    }

    /// Filtra los drivers cercanos a una posicion dada.
    fn filter_nearby_drivers(
        driver_positions: &HashMap<u32, Position>,
        source: &Position,
    ) -> Vec<u32> {
        let mut distances = driver_positions
            .clone()
            .into_par_iter()
            .map(|(k, v)| (k, v.distance_to(&source)))
            .filter(|(_, v)| *v <= MAX_DISTANCE)
            .collect::<Vec<(u32, u32)>>();

        distances.par_sort_by(|(_, a), (_, b)| a.cmp(&b));

        let nearby_drivers = distances
            .into_par_iter()
            .map(|(k, _)| k)
            .collect::<Vec<u32>>();

        nearby_drivers
    }


    /// Busca el driver que mejor se ajuste al pasajero que solicito el viaje.
    /// Obtiene primero los drivers que se encuentren un radio válido al pasajero.
    /// Parsea el mensaje a enviar a los drivers válidos.
    /// Itera por cada uno de los drivers con la siguiente lógica:
    /// - Si el driver válido es el mismo que está ejecutando la búsqueda, envia un mensaje al actor `TripHandler` para que maneje el viaje (sin necesidad de comunicarse por el stream).
    /// - Si el driver válido no es si mismo, le envía un mensaje "SendAll" al driver (comunicandose por el stream de escritura) con el mensaje parseado.
    /// - Espera un tiempo para chequear si hubo respuesta por parte del driver.
    /// - Luego se envía un mensaje "CheckACK" al driver para verificar si el pasajero envio un ACK. En el caso que de que hubo respuesta del pasajero
    /// arranca el viaje.
    /// - Si el driver no responde, loggea un mensaje de error.
    /// - En el caso de que ningún driver esté a un radio cercano del pasajero que solicito el viaje, se envía un mensaje al actor `CentralDriver` para que se conecte con el pasajero y le envía un mensaje "SendTripResponse" al pasajero con el estado del viaje (Error) y un detalle.

    async fn find_driver(
        driver_positions: &HashMap<u32, Position>,
        id: &u32,
        connection_with_drivers: &HashMap<u32, Addr<DriverConnection>>,
        msg: &FindDriver,
        trip_handler: &Addr<TripHandler>,
        self_addr: &Addr<Self>,
    ) {
        let nearby_drivers = Self::filter_nearby_drivers(driver_positions, &msg.source);

        log::debug!(
            "[TRIP] Nearby drivers for passenger {}: {:?}",
            msg.passenger_id,
            nearby_drivers
        );

        let parsed_data = serde_json::to_string(&DriverMessages::CanHandleTrip {
            passenger_id: msg.passenger_id,
            passenger_location: msg.source,
            destination: msg.destination,
        })
        .inspect_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        });

        if let Ok(data) = &parsed_data {
            for did in nearby_drivers {
                log::info!(
                    "[TRIP] Asking driver {} if it will take the trip for passenger {}",
                    did,
                    msg.passenger_id
                );
                if did == *id {
                    let res = trip_handler
                        .send(super::handle_trip::CanHandleTrip {
                            passenger_id: msg.passenger_id,
                            passenger_location: msg.source,
                            destination: msg.destination,
                            self_id: *id,
                        })
                        .await
                        .map_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                            e.to_string()
                        });

                    if let Ok(value) = res {
                        if value {
                            log::info!(
                                "[TRIP] Driver {} will take the trip for passenger {}",
                                did,
                                msg.passenger_id
                            );

                            return;
                        }
                    }
                }

                if let Some(driver) = connection_with_drivers.get(&did) {
                    let _ = driver
                        .try_send(SendAll { data: data.clone() })
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        });

                    sleep(TAKE_TRIP_TIMEOUT_MS).await;

                    let res = driver
                        .send(CheckACK {
                            passenger_id: msg.passenger_id,
                        })
                        .await
                        .map_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                            e.to_string()
                        });

                    if let Ok(ack) = res {
                        if let Some(value) = ack {
                            if value {
                                log::info!(
                                    "[TRIP] Driver {} will take the trip for passenger {}",
                                    did,
                                    msg.passenger_id
                                );

                                return;
                            }
                        }
                    }
                }

                log::debug!(
                    "[TRIP] Driver {} can not take the trip or did not answer",
                    did
                )
            }
        }

        let detail = format!("There are no drivers available near your location");

        log::info!("There are no drivers near passenger {}", msg.passenger_id);

        match self_addr
            .send(ConnectWithPassenger {
                passenger_id: msg.passenger_id,
            })
            .await
        {
            Ok(Ok(_)) => {
                let _ = self_addr
                    .try_send(SendTripResponse {
                        status: TripStatus::Error,
                        detail,
                        passenger_id: msg.passenger_id,
                    })
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    });
            }
            _ => {
                log::error!("Can not connect with passenger {}", msg.passenger_id);
            }
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct NotifyPositionToLeader {
    pub driver_location: Position,
}

impl Handler<NotifyPositionToLeader> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de notificacion de posicion de un driver.
    /// - Si no tengo el lider, no hago nada.
    /// - Si soy el lider, envio un mensaje al actor `CentralDriver` para que actualice la posicion del driver.
    /// - Si no soy el lider, envio un mensaje al lider con la posicion del driver.
    fn handle(&mut self, msg: NotifyPositionToLeader, ctx: &mut Context<Self>) -> Self::Result {
        if self.leader_id.is_none() {
            return;
        }

        if let Some(lid) = self.leader_id {
            if self.im_leader() {
                ctx.address().do_send(SetDriverPosition {
                    driver_id: self.id,
                    driver_position: msg.driver_location,
                });

                return;
            }

            let parsed_data = serde_json::to_string(&DriverMessages::NotifyPosition {
                driver_id: self.id,
                driver_position: msg.driver_location,
            })
            .inspect_err(|e| log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string()));

            if let Ok(data) = parsed_data {
                match self.connection_with_drivers.get(&lid) {
                    Some(leader) => leader.do_send(SendAll { data }),
                    None => (),
                };
            }
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SetDriverPosition {
    /// Id del driver
    pub driver_id: u32,
    /// Posicion del driver
    pub driver_position: Position,
}

impl Handler<SetDriverPosition> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de actualizacion de posicion de un driver.
    /// Actualiza la posicion del driver en el hashmap de posiciones de drivers.
    /// Loggea la posicion del driver.
    fn handle(&mut self, msg: SetDriverPosition, _ctx: &mut Context<Self>) -> Self::Result {
        log::debug!("Driver {} in {:?}", msg.driver_id, msg.driver_position);
        self.driver_positions
            .insert(msg.driver_id, msg.driver_position);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CollectMoneyPassenger {
    /// Id del pasajero
    pub passenger_id: u32,
}

#[async_handler]
impl Handler<CollectMoneyPassenger> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de cobro de un pasajero.
    /// - Se conecta con el servicio de pagos.
    /// - Si se conecta correctamente, envia un mensaje al servicio de pagos con el id del driver y el id del pasajero.
    /// - Si no se conecta correctamente, loggea un error.
    async fn handle(
        &mut self,
        msg: CollectMoneyPassenger,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        let driver_id = self.id.clone();
        let self_addr = _ctx.address().clone();

        let connection_with_payment = PaymentConnection::connect(self_addr).await;

        if let Ok(addr) = connection_with_payment {
            let parsed_data = serde_json::to_string(&PaymentMessages::CollectPayment {
                driver_id,
                passenger_id: msg.passenger_id,
            })
            .inspect_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            });

            if let Ok(data) = parsed_data {
                let _ = addr
                    .try_send(super::payment_connection::SendAll { data })
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    });
            }
        }

        ()
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CheckPaymentResponse {
    pub passenger_id: u32,
    pub response: bool,
}

impl Handler<CheckPaymentResponse> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de respuesta de cobro de un pasajero.
    /// - Si el pasajero pago, loggea un mensaje de que el pasajero pago.
    /// - Si el pasajero no pago, loggea un mensaje de que el pasajero no pago.
    fn handle(&mut self, msg: CheckPaymentResponse, _ctx: &mut Context<Self>) -> Self::Result {
        match msg.response {
            true => log::info!("Passenger {} paid for the trip!", msg.passenger_id),
            _ => log::warn!(
                "Passenger {} did not pay for the trip!!, call the police!",
                msg.passenger_id
            ),
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RemovePassengerConnection {
    /// Id del pasajero
    pub id: u32,
}

impl Handler<RemovePassengerConnection> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de eliminacion de conexion de un pasajero.
    /// - Elimina la conexion del pasajero del hashmap de pasajeros.
    /// - Loggea un mensaje de desconexion con el pasajero.
    /// - Envía un mensaje al actor `TripHandler` para que maneje la eliminacion del pasajero.
    fn handle(&mut self, msg: RemovePassengerConnection, ctx: &mut Context<Self>) -> Self::Result {
        let passenger = self.passengers.clone();
        let trip_handler = self.trip_handler.clone();

        wrap_future::<_, Self>(async move {
            let mut lock = passenger.lock().await;

            if let Some(_) = (*lock).remove(&msg.id) {
                log::info!("Disconnecting with passenger {}", msg.id);
                let _ = trip_handler
                    .try_send(ClearPassenger {
                        disconnected: true,
                        passenger_id: msg.id,
                    })
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                    });
            }
        })
        .spawn(ctx);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct InsertDriverConnection {
    /// Id del driver
    pub id: u32,
    /// Direccion del actor `DriverConnection`
    pub addr: Addr<DriverConnection>,
}

impl Handler<InsertDriverConnection> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de conexion con un driver.
    /// - Loggea un mensaje de conexion con el driver.
    /// - Inserta la conexion del driver en el hashmap de conexiones con drivers.
    fn handle(&mut self, msg: InsertDriverConnection, _ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Connecting with driver {}", msg.id);
        self.connection_with_drivers.insert(msg.id, msg.addr);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RemoveDriverConnection {
    /// Id del driver
    pub id: u32,
}

impl Handler<RemoveDriverConnection> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de eliminacion de conexion con un driver.
    /// - Elimina la conexion del driver del hashmap de conexiones con drivers.
    /// - Loggea un mensaje de desconexion con el driver.
    fn handle(&mut self, msg: RemoveDriverConnection, _ctx: &mut Context<Self>) -> Self::Result {
        if let Some(_) = self.connection_with_drivers.remove(&msg.id) {
            log::info!("Disconnecting with driver {}", msg.id);
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct StartElection {}

impl Handler<StartElection> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de inicio de eleccion.
    /// Envia el mensaje Election a todos los drivers con id mayor al id propio.
    /// - Si no hay drivers con mayor id que el propio se declara coordinador y le notifica a todos los conductores.
    /// - Si hay drivers con mayor id que el propio, les envía el mensaje Election.
    ///     - Setea un timeout para la eleccion.
    ///     - Si no hay respuesta de los drivers con mayor id que el propio, se declara coordinador y le notifica a todos los conductores.
    fn handle(&mut self, _msg: StartElection, ctx: &mut Context<Self>) -> Self::Result {
        self.leader_id = None;
        let mut higher_processes = false;

        let parsed_data = serde_json::to_string(&DriverMessages::Election { sender_id: self.id })
            .inspect_err(|e| log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string()));

        // Send election messages to all processes with higher IDs
        for (&id, driver) in &self.connection_with_drivers {
            if id > self.id {
                if let Ok(data) = &parsed_data {
                    driver.do_send(SendAll { data: data.clone() });
                }

                higher_processes = true;
            }
        }

        // If no higher processes, declare Coordinator
        if !higher_processes {
            log::info!("[ELECTION] There is no one bigger than me!");
            ctx.notify(Coordinator { leader_id: self.id });

            for (_, driver) in &self.connection_with_drivers {
                let parsed_data =
                    serde_json::to_string(&DriverMessages::Coordinator { leader_id: self.id })
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                        });

                if let Ok(data) = parsed_data {
                    driver.do_send(SendAll { data });
                }
            }
        } else {
            let leader_id = self.id.clone();
            let connection_with_drivers = self.connection_with_drivers.clone();
            
            // Set timeout for responses
            self.election_timeout =
                Some(ctx.run_later(ELECTION_TIMEOUT_DURATION, move |_, ctx| {

                    log::warn!("[ELECTION] No one answer the election");
                    ctx.notify(Coordinator { leader_id });

                    for (_, driver) in &connection_with_drivers {
                        let parsed_data =
                            serde_json::to_string(&DriverMessages::Coordinator { leader_id })
                                .inspect_err(|e| {
                                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                                });

                        if let Ok(data) = parsed_data {
                            driver.do_send(SendAll { data });
                        }
                    }
                }));
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Election {
    pub sender_id: u32,
}

impl Handler<Election> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de eleccion.
    /// - Si el driver recibe un mensaje de eleccion, verifica si el id del driver que envia el mensaje es menor al id propio.
    ///
    /// Si esto sucede envia un mensaje "Alive" al driver que envia el mensaje y envia un mensaje "StartElection" al actor `CentralDriver`
    /// para continuar con el proceso de elección.

    fn handle(&mut self, msg: Election, ctx: &mut Context<Self>) -> Self::Result {
        log::debug!(
            "[ELECTION] Driver {} received election message from {}",
            self.id,
            msg.sender_id
        );

        // If this process has higher ID, respond and start new election
        if self.id > msg.sender_id {
            // Send alive message to sender
            if let Some(sender) = self.connection_with_drivers.get(&msg.sender_id) {
                let parsed_data = serde_json::to_string(&DriverMessages::Alive {
                    responder_id: self.id,
                })
                .inspect_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                });

                if let Ok(data) = parsed_data {
                    sender.do_send(SendAll { data });
                }
            }

            // Start new election
            ctx.notify(StartElection {});
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Alive {
    pub responder_id: u32,
}

impl Handler<Alive> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de respuesta a una eleccion.
    /// - Si el driver recibe un mensaje de respuesta, cancela el timeout de la eleccion.
    /// - Loggea un mensaje de que el driver recibio un mensaje de respuesta.
    fn handle(&mut self, msg: Alive, ctx: &mut Context<Self>) -> Self::Result {
        log::debug!(
            "[ELECTION] Driver {} received alive message from {}",
            self.id,
            msg.responder_id
        );

        if let Some(timeout) = self.election_timeout {
            ctx.cancel_future(timeout);
            self.election_timeout = None;
        }
        // Cancel election timeout as we received a response
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Coordinator {
    pub leader_id: u32,
}

impl Handler<Coordinator> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de coordinador.
    /// - Loggea un mensaje de que el driver recibio un mensaje de coordinador.
    /// - Setea el id del lider con el id del driver que envia el mensaje.
    /// - Si el driver es el lider, loggea un mensaje de que el driver es el lider.
    /// - Envía un mensaje al actor `TripHandler` para que notifique la posicion del driver.
    fn handle(&mut self, msg: Coordinator, _ctx: &mut Context<Self>) -> Self::Result {
        log::info!("[ELECTION] {} is the new leader", msg.leader_id);

        self.leader_id = Some(msg.leader_id);

        if self.im_leader() {
            log::info!("[ELECTION] Oh!, that is me");
        }

        let _ = self
            .trip_handler
            .try_send(ForceNotifyPosition {})
            .inspect_err(|e| log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string()));
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct RedirectNewTrip {
    pub passenger_id: u32,
    pub source: Position,
    pub destination: Position,
}

impl Handler<RedirectNewTrip> for CentralDriver {
    type Result = Result<(), String>;

    /// Maneja los mensajes de redireccion de un nuevo viaje.
    /// - Si el driver es el lider, envia un mensaje  `FindDriver` al actor con el id del pasajero, la posicion de origen y la posicion de destino.
    /// - Si el driver no es el lider, envia un mensaje "TripRequest" al lider con el id del pasajero, la posicion de origen y la posicion de destino.
    fn handle(&mut self, msg: RedirectNewTrip, ctx: &mut Context<Self>) -> Self::Result {
        log::debug!(
            "[TRIP] Redirect to leader a trip for passenger {}",
            msg.passenger_id
        );

        if let Some(lid) = &self.leader_id {
            if self.im_leader() {
                ctx.address()
                    .try_send(FindDriver {
                        passenger_id: msg.passenger_id,
                        source: msg.source,
                        destination: msg.destination,
                    })
                    .map_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        e.to_string()
                    })?;
            } else {
                let leader_addr = self.connection_with_drivers.get(lid);

                if let Some(laddr) = leader_addr {
                    let data = serde_json::to_string(&DriverMessages::TripRequest {
                        passenger_id: msg.passenger_id,
                        passenger_location: msg.source,
                        destination: msg.destination,
                    })
                    .map_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        e.to_string()
                    })?;

                    laddr.try_send(SendAll { data }).map_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        e.to_string()
                    })?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct FindDriver {
    pub passenger_id: u32,
    pub source: Position,
    pub destination: Position,
}

impl Handler<FindDriver> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de busqueda de un driver.
    /// Genera una tarea asincrónica para buscar un driver para un pasajero.
    fn handle(&mut self, msg: FindDriver, ctx: &mut Context<Self>) -> Self::Result {
        if !self.im_leader() {
            return;
        }

        log::debug!("[TRIP] Finding a driver for passenger {}", msg.passenger_id);

        let driver_positions = self.driver_positions.clone();
        let id = self.id.clone();
        let connection_with_drivers = self.connection_with_drivers.clone();
        let trip_handler = self.trip_handler.clone();
        let self_addr = ctx.address().clone();

        wrap_future::<_, Self>(async move {
            CentralDriver::find_driver(
                &driver_positions,
                &id,
                &connection_with_drivers,
                &msg,
                &trip_handler,
                &self_addr,
            )
            .await;
        })
        .spawn(ctx);

        ()
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct CanHandleTrip {
    pub passenger_id: u32,
    pub source: Position,
    pub destination: Position,
}

impl Handler<CanHandleTrip> for CentralDriver {
    type Result = Result<(), String>;

    /// Maneja los mensajes de si puede manejar un viaje.
    /// Manda un mensaje al actor `TripHandler` para validar si puede  manejar el viaje y espera la respuesta.
    /// - Si no hay ningun error le envia al lider el mensaje CanHandleTripACK con el id del cliente y
    /// el estado del viaje
    fn handle(&mut self, msg: CanHandleTrip, ctx: &mut Context<Self>) -> Self::Result {
        let leader_id = self.leader_id.clone();
        let connection_with_drivers = self.connection_with_drivers.clone();
        let trip_handler = self.trip_handler.clone();
        let self_id = self.id.clone();

        wrap_future::<_, Self>(async move {
            let res = trip_handler
                .send(super::handle_trip::CanHandleTrip {
                    passenger_id: msg.passenger_id,
                    passenger_location: msg.source,
                    destination: msg.destination,
                    self_id,
                })
                .await
                .inspect_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                })
                .unwrap_or(false);

            if let Some(lid) = &leader_id {
                if let Some(leader) = connection_with_drivers.get(lid) {
                    let parsed_data = serde_json::to_string(&DriverMessages::CanHandleTripACK {
                        response: res,
                        passenger_id: msg.passenger_id,
                    })
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    });

                    if let Ok(data) = parsed_data {
                        let _ = leader.try_send(SendAll { data }).inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        });
                    }
                }
            }
        })
        .spawn(ctx);

        Ok(())
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct ConnectWithPassenger {
    pub passenger_id: u32,
}

#[async_handler]
impl Handler<ConnectWithPassenger> for CentralDriver {
    type Result = Result<(), String>;


    /// Maneja los mensajes de conexion con un pasajero.
    /// Se conecta con el actor `PassengerConnection` y envia un mensaje "Connect" con el id del pasajero y su dirección
    /// - Si se conecta correctamente, loggea un mensaje de conexion con el pasajero.
    /// - Si no se conecta correctamente, loggea un mensaje de error.
    /// - Si se conecta correctamente, inserta la conexion del pasajero en el hashmap de pasajeros.
    async fn handle(
        &mut self,
        msg: ConnectWithPassenger,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        let passenger = self.passengers.clone();
        let self_addr = _ctx.address();

        let passenger_addr =
            PassengerConnection::connect(self_addr.clone(), msg.passenger_id).await;

        let mut lock = passenger.lock_owned().await;

        match passenger_addr {
            Ok(addr) => {
                log::info!("Connecting with passenger {}", msg.passenger_id);

                (*lock).insert(msg.passenger_id, addr);

                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendTripResponse {
    /// Estado del viaje
    pub status: TripStatus,
    /// Detalle del viaje
    pub detail: String,
    /// Id del pasajero
    pub passenger_id: u32,
}

impl Handler<SendTripResponse> for CentralDriver {
    type Result = ();

    /// Maneja los mensajes de respuesta de un viaje.
    /// - Si el driver recibe un mensaje de respuesta, envia un mensaje al pasajero con el estado del viaje y el detalle.
    /// - Si el driver no recibe un mensaje de respuesta, loggea un mensaje de error.

    fn handle(&mut self, msg: SendTripResponse, ctx: &mut Context<Self>) -> Self::Result {
        let parsed_data = serde_json::to_string(&TripMessages::TripResponse {
            status: msg.status,
            detail: msg.detail.clone(),
        })
        .inspect_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        });

        let passenger = self.passengers.clone();

        wrap_future::<_, Self>(async move {
            let lock = passenger.lock().await;

            if let Ok(data) = parsed_data {
                if let Some(paddr) = (*lock).get(&msg.passenger_id) {
                    let _ = paddr
                        .try_send(super::passenger_connection::SendAll { data })
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        });
                }
            }
        })
        .spawn(ctx);
    }
}

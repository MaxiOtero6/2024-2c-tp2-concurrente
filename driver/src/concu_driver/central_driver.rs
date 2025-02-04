use std::collections::HashMap;

use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, SpawnHandle};
use actix_async_handler::async_handler;
use common::utils::{
    json_parser::{PaymentMessages, TripMessages, TripStatus},
    position::Position,
};

use crate::concu_driver::{
    driver_connection::SendAll,
    handle_trip::{ClearPassenger, ForceNotifyPosition},
    json_parser::DriverMessages,
};

use super::{
    consts::ELECTION_TIMEOUT_DURATION,
    driver_connection::DriverConnection,
    driver_finder::{DriverACK, DriverFinder},
    handle_trip::TripHandler,
    passenger_connection::PassengerConnection,
    payment_connection::PaymentConnection,
};

pub struct CentralDriver {
    /// Direccion del actor TripHandler
    trip_handler: Addr<TripHandler>,
    /// Direccion del actor PassengerConnection
    passengers: HashMap<u32, Addr<PassengerConnection>>,
    /// Direcciones de los buscadores de drivers segun la id del pasajero
    driver_finders: HashMap<u32, Addr<DriverFinder>>,
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
            passengers: HashMap::new(),
            election_timeout: None,
            driver_finders: HashMap::new(),
        })
    }

    /// Verifica si el driver es el lider a partir de su id.
    fn im_leader(&self) -> bool {
        if let Some(lid) = self.leader_id {
            return self.id == lid;
        }

        false
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
    fn handle(&mut self, msg: RemovePassengerConnection, _ctx: &mut Context<Self>) -> Self::Result {
        let trip_handler = self.trip_handler.clone();

        if let Some(_) = self.passengers.remove(&msg.id) {
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
                                    log::error!(
                                        "{}:{}, {}",
                                        std::file!(),
                                        std::line!(),
                                        e.to_string()
                                    )
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
    /// Genera un actor DriverFinder y lo inicia para buscar un driver a un pasajero.
    fn handle(&mut self, msg: FindDriver, ctx: &mut Context<Self>) -> Self::Result {
        if !self.im_leader() {
            return;
        }

        log::debug!("[TRIP] Finding a driver for passenger {}", msg.passenger_id);

        self.driver_finders.insert(
            msg.passenger_id,
            DriverFinder::new(
                ctx.address().clone(),
                msg.passenger_id,
                msg.source,
                msg.destination,
                self.driver_positions.clone(),
            )
            .start(),
        );
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CanHandleTrip {
    pub passenger_id: u32,
    pub source: Position,
    pub destination: Position,
    pub driver_id: u32,
}

impl Handler<CanHandleTrip> for CentralDriver {
    type Result = ();

    /// Redirige la consulta para tomar un viaje. En caso de querer consultar a el mismo, redirige la consulta
    /// al TripHandler, en cambio, envia el mensaje al DriverConnection indicado.
    fn handle(&mut self, msg: CanHandleTrip, _ctx: &mut Context<Self>) -> Self::Result {
        if msg.driver_id == self.id {
            let _ = self
                .trip_handler
                .try_send(super::handle_trip::CanHandleTrip {
                    passenger_id: msg.passenger_id,
                    passenger_location: msg.source,
                    destination: msg.destination,
                    self_id: self.id,
                })
                .inspect_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                });

            return;
        }

        if let Some(driver) = self.connection_with_drivers.get(&msg.driver_id) {
            let parsed_data = serde_json::to_string(&DriverMessages::CanHandleTrip {
                passenger_location: msg.source,
                passenger_id: msg.passenger_id,
                destination: msg.destination,
                driver_id: msg.driver_id,
            })
            .inspect_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            });

            if let Ok(data) = parsed_data {
                let _ = driver.try_send(SendAll { data }).inspect_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                });
            }
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CanHandleTripACK {
    pub passenger_id: u32,
    pub response: bool,
    pub driver_id: u32,
}

impl Handler<CanHandleTripACK> for CentralDriver {
    type Result = ();

    /// Redirige al DriverFinder que consulto acerca de tomar el viaje.
    fn handle(&mut self, msg: CanHandleTripACK, _ctx: &mut Context<Self>) -> Self::Result {
        if self.im_leader() {
            if let Some(df) = self.driver_finders.get(&msg.passenger_id) {
                let _ = df
                    .try_send(DriverACK {
                        response: msg.response,
                        driver_id: msg.driver_id,
                    })
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    });
            }

            return;
        }

        if let Some(lid) = &self.leader_id {
            if let Some(leader) = self.connection_with_drivers.get(lid) {
                let parsed_data = serde_json::to_string(&DriverMessages::CanHandleTripACK {
                    response: msg.response,
                    passenger_id: msg.passenger_id,
                    driver_id: self.id,
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
        let self_addr = _ctx.address();

        let passenger_addr =
            PassengerConnection::connect(self_addr.clone(), msg.passenger_id).await;

        match passenger_addr {
            Ok(addr) => {
                log::info!("Connecting with passenger {}", msg.passenger_id);

                self.passengers.insert(msg.passenger_id, addr);

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

    fn handle(&mut self, msg: SendTripResponse, _ctx: &mut Context<Self>) -> Self::Result {
        let parsed_data = serde_json::to_string(&TripMessages::TripResponse {
            status: msg.status,
            detail: msg.detail.clone(),
        })
        .inspect_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        });

        if let Ok(data) = parsed_data {
            if let Some(paddr) = self.passengers.get(&msg.passenger_id) {
                let _ = paddr
                    .try_send(super::passenger_connection::SendAll { data })
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    });
            }
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RemoveDriverFinder {
    /// Id del pasajero
    pub passenger_id: u32,
}

impl Handler<RemoveDriverFinder> for CentralDriver {
    type Result = ();

    /// Elimina un DriverFinder si este existe
    ///  - passenger_id: ID del pasajero que pidio el viaje

    fn handle(&mut self, msg: RemoveDriverFinder, _ctx: &mut Context<Self>) -> Self::Result {
        if let Some(_) = self.driver_finders.remove(&msg.passenger_id) {
            log::info!("Removing driver finder {}", msg.passenger_id);
        }
    }
}

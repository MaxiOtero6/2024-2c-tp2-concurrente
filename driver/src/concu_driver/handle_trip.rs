use crate::concu_driver::{
    central_driver::{CollectMoneyPassenger, SendTripResponse},
    consts::{DEFAULT_TAKE_TRIP_PROBABILTY, TRIP_GO_TO_SLEEP},
};
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message};
use actix_async_handler::async_handler;
use common::utils::{json_parser::TripStatus, position::Position};
use rand::Rng;

use super::{
    central_driver::{CentralDriver, ConnectWithPassenger, NotifyPositionToLeader},
    consts::POSITION_NOTIFICATION_INTERVAL,
};

pub struct TripHandler {
    /// Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    /// Posicion actual del driver
    current_location: Option<Position>,
    /// Id del pasajero actual
    passenger_id: Option<u32>,
    // Variable de entorno 'TEST' para simplificar casos de interes a la hora de testear
    test_env_var: Result<String, std::env::VarError>,
}

impl Actor for TripHandler {
    type Context = Context<Self>;

    /// Inicializa el actor notificando su posicion al CentralDriver
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.notify(NotifyPosition {});
    }
}

impl TripHandler {
    /// Crea una nueva conexión con un driver con:
    /// - La dirección del actor `CentralDriver`
    /// - El ID del driver.
    /// - El valor de la variable de entorno TEST, si esta esta definida
    /// - Una posicion inicial aleatoria o fija en caso de que este definida la variable de entorno TEST
    pub fn new(central_driver: Addr<CentralDriver>, self_id: u32) -> Self {
        let test_env_var: Result<String, std::env::VarError> = std::env::var("TEST");

        let pos = match test_env_var {
            Ok(_) => Position::new(self_id * 5, self_id * 5),
            Err(_) => Position::random(),
        };

        Self {
            central_driver,
            current_location: Some(pos),
            passenger_id: None,
            test_env_var,
        }
    }

    /// Simula la posición del driver y la notifica al `CentralDriver`.
    fn notify_pos(&mut self) {
        if let Some(mut position) = self.current_location.take() {
            // Simulate position change
            match self.test_env_var {
                Ok(_) => (),
                Err(_) => position.simulate(),
            }

            let _ = self
                .central_driver
                .try_send(NotifyPositionToLeader {
                    driver_location: position.clone(),
                })
                .inspect_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                });

            self.current_location = Some(position);
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct GoTo {
    /// Posicion actual del driver
    current_position: Position,
    /// Posicion destino siguiente
    next_position: Position,
    /// Id del pasajero
    passenger_id: u32,
    /// Posicion inicial del pasajero
    passenger_location: Position,
    /// Posicion destino del pasajero
    destination: Position,
}

impl Handler<GoTo> for TripHandler {
    type Result = ();

    /// - Simula un viaje, luego de una simulacion se encola nuevamente este mensaje para continuar la simulacion
    ///     hasta el destino del pasajero
    /// - Se mueve hasta la posición del pasajero.
    /// - Le notifica al Central Driver que llego a la posición del pasajero
    /// - Se mueve hasta la posición de destino.
    /// - Le notifica al Central Driver que llego a la posición destino
    /// - Le notifica al Central Driver para que solicite el cobro del viaje realizado
    /// - Limpia el estado del viaje.
    fn handle(&mut self, msg: GoTo, ctx: &mut Context<Self>) -> Self::Result {
        if let None = self.passenger_id {
            self.current_location = Some(msg.current_position);
            return;
        }

        let mut current_position = msg.current_position;
        current_position.go_to(&msg.next_position);
        log::debug!(
            "Current {:?} -> Destination {:?}",
            current_position,
            msg.next_position
        );

        if current_position == msg.passenger_location {
            if current_position != msg.current_position {
                let _ = self
                    .central_driver
                    .try_send(SendTripResponse {
                        passenger_id: msg.passenger_id,
                        status: TripStatus::Info,
                        detail: format!("I am at your door, come out!"),
                    })
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                    });

                log::info!("[TRIP] Passenger {} picked up", msg.passenger_id);
            }

            ctx.notify_later(
                GoTo {
                    current_position,
                    next_position: msg.destination,
                    passenger_id: msg.passenger_id,
                    passenger_location: msg.passenger_location,
                    destination: msg.destination,
                },
                TRIP_GO_TO_SLEEP,
            );

            return;
        } else if current_position == msg.destination {
            log::info!(
                "[TRIP] Arrived at destination for passenger {}",
                msg.passenger_id
            );

            let detail =
                format!("We have arrived at our destination, we hope you enjoyed the trip");

            let _ = self
                .central_driver
                .try_send(SendTripResponse {
                    passenger_id: msg.passenger_id,
                    status: TripStatus::Success,
                    detail,
                })
                .inspect_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                });

            let _ = self
                .central_driver
                .try_send(CollectMoneyPassenger {
                    passenger_id: msg.passenger_id,
                })
                .inspect_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                });

            ctx.notify(ClearPassenger {
                disconnected: false,
                passenger_id: msg.passenger_id,
            });

            self.current_location = Some(current_position);

            return;
        }

        ctx.notify_later(
            GoTo {
                current_position,
                next_position: msg.next_position,
                passenger_id: msg.passenger_id,
                passenger_location: msg.passenger_location,
                destination: msg.destination,
            },
            TRIP_GO_TO_SLEEP,
        );
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CanHandleTrip {
    /// Id del pasajero
    pub passenger_id: u32,
    /// Posicion inicial del pasajero
    pub passenger_location: Position,
    /// Posicion destino del pasajero
    pub destination: Position,
    /// Id de este driver
    pub self_id: u32,
}

#[async_handler]
impl Handler<CanHandleTrip> for TripHandler {
    type Result = ();

    /// Maneja los mensajes recibidos desde el pasajero.
    /// Simula la situación de si el driver puede tomar el viaje o no.
    /// - Si el driver puede tomar el viaje, se conecta con el Central Driver y le envia el mensaje `ConnectWithPassenger` para que se conecte con el pasajero.
    ///     - Si la conexión fue exitosa, le envia un mensaje al Central Driver con el mensaje `SendTripResponse` para notificarle al pasajero que el driver esta en camino.
    ///     - Ademas de notificarle que se encuentra en el 'infinito' con el fin de que no sea tomado en cuenta para proximos viajes
    ///     - Inicia el viaje enviando un mensaje al actor con el mensaje `GoTo`.
    /// - Si el driver no puede tomar el viaje, retorna `false` al CentralDriver con el mensaje 'CanHandleTripACK'.
    async fn handle(&mut self, msg: CanHandleTrip, _ctx: &mut Context<Self>) -> Self::Result {
        let mut rng = rand::thread_rng();
        let response = self.passenger_id.is_none()
            && rng.gen_bool(
                std::env::var("TAKE_TRIP_PROBABILITY")
                    .unwrap_or(DEFAULT_TAKE_TRIP_PROBABILTY.to_string())
                    .parse()
                    .unwrap_or(DEFAULT_TAKE_TRIP_PROBABILTY),
            );

        let pid = msg.passenger_id.clone();
        let did = msg.self_id.clone();

        let res = if response {
            let result = self
                .central_driver
                .send(ConnectWithPassenger {
                    passenger_id: msg.passenger_id,
                })
                .await;

            match result {
                Ok(Ok(_)) => {
                    let _ = self
                        .central_driver
                        .try_send(SendTripResponse {
                            passenger_id: msg.passenger_id,
                            status: TripStatus::Info,
                            detail: format!(
                                "Hi!, i am driver {}. I will be at your location in a moment.",
                                msg.self_id
                            ),
                        })
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                        });

                    let p = self.current_location.take();

                    if let Some(current_position) = p {
                        self.passenger_id = Some(msg.passenger_id);

                        let _ = self
                            .central_driver
                            .try_send(NotifyPositionToLeader {
                                driver_location: Position::infinity(),
                            })
                            .inspect(|_| log::debug!("Sent infinity!"))
                            .inspect_err(|e| {
                                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                            });

                        log::info!("[TRIP] Start trip for passenger {}", msg.passenger_id);

                        _ctx.notify(GoTo {
                            current_position,
                            next_position: msg.passenger_location,
                            passenger_id: msg.passenger_id,
                            passenger_location: msg.passenger_location,
                            destination: msg.destination,
                        });

                        true
                    } else {
                        log::error!(
                            "{}:{}, {}",
                            std::file!(),
                            std::line!(),
                            "Why im here if i dont have a position in the world!!!"
                        );

                        false
                    }
                }
                _ => false,
            }
        } else {
            false
        };

        let _ = self
            .central_driver
            .try_send(super::central_driver::CanHandleTripACK {
                passenger_id: pid,
                response: res,
                driver_id: did,
            })
            .inspect_err(|e| log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string()));
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ClearPassenger {
    /// Si el pasajero se desconecto
    pub disconnected: bool,
    /// Id del pasajero
    pub passenger_id: u32,
}

impl Handler<ClearPassenger> for TripHandler {
    type Result = ();

    /// Limpia el estado del viaje.
    fn handle(&mut self, msg: ClearPassenger, _ctx: &mut Context<Self>) -> Self::Result {
        if let None = self.passenger_id {
            return ();
        }

        if self.passenger_id.unwrap() == msg.passenger_id {
            if msg.disconnected {
                log::warn!("What the hell!! The passenger jump out of the car!!");
            }

            self.passenger_id = None;
            log::info!("Now i'm ready for another trip!");
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ForceNotifyPosition {}
impl Handler<ForceNotifyPosition> for TripHandler {
    type Result = ();

    /// Notifica la posición al `CentralDriver`.
    fn handle(&mut self, _msg: ForceNotifyPosition, _ctx: &mut Context<Self>) -> Self::Result {
        self.notify_pos();
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct NotifyPosition {}
impl Handler<NotifyPosition> for TripHandler {
    type Result = ();

    /// Notifica la posición al `CentralDriver` y se asegura que se vuelva a llamar a este mensaje en
    /// POSITION_NOTIFICATION_INTERVAL segundos
    fn handle(&mut self, _msg: NotifyPosition, ctx: &mut Context<Self>) -> Self::Result {
        self.notify_pos();

        ctx.notify_later(NotifyPosition {}, POSITION_NOTIFICATION_INTERVAL);
    }
}

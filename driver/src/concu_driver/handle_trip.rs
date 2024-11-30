use std::{sync::Arc, time::Duration};

use crate::concu_driver::{
    central_driver::{CollectMoneyPassenger, SendTripResponse},
    consts::TAKE_TRIP_PROBABILTY,
};
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, SpawnHandle, WrapFuture};
use actix_async_handler::async_handler;
use common::utils::{json_parser::TripStatus, position::Position};
use rand::Rng;
use tokio::{sync::Mutex, time::sleep};

use super::{
    central_driver::{CentralDriver, ConnectWithPassenger, NotifyPositionToLeader},
    consts::POSITION_NOTIFICATION_INTERVAL,
};

pub struct TripHandler {
    // Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    // Posicion actual del driver
    current_location: Arc<Mutex<Position>>,
    // Id del pasajero actual
    passenger_id: Option<u32>,
    // Tarea del viaje
    trip_task: Option<SpawnHandle>,
}

impl Actor for TripHandler {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let recipient = self.central_driver.clone();

        ctx.spawn({
            let position_lock: Arc<Mutex<Position>> = Arc::clone(&self.current_location);
            let test_env_var: Result<String, std::env::VarError> = std::env::var("TEST");

            async move {
                loop {
                    Self::notify_pos(&recipient, &position_lock, &test_env_var).await;

                    sleep(POSITION_NOTIFICATION_INTERVAL).await;
                }
            }
            .into_actor(self)
        });
    }
}

impl TripHandler {
    pub fn new(central_driver: Addr<CentralDriver>, self_id: u32) -> Self {
        let pos = match std::env::var("TEST") {
            Ok(_) => Position::new(self_id * 5, self_id * 5),
            Err(_) => Position::random(),
        };

        Self {
            central_driver,
            current_location: Arc::new(Mutex::new(pos)),
            passenger_id: None,
            trip_task: None,
        }
    }

    async fn notify_pos(
        central_driver: &Addr<CentralDriver>,
        position_lock: &Arc<Mutex<Position>>,
        test_env_var: &Result<String, std::env::VarError>,
    ) {
        let mut lock = position_lock.lock().await;
        // Simulate position change
        match test_env_var {
            Ok(_) => (),
            Err(_) => (*lock).simulate(),
        }

        let _ = central_driver
            .try_send(NotifyPositionToLeader {
                driver_location: *lock,
            })
            .inspect_err(|e| log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string()));
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct TripStart {
    passenger_id: u32,
    passenger_location: Position,
    destination: Position,
}
impl Handler<TripStart> for TripHandler {
    type Result = ();

    fn handle(&mut self, msg: TripStart, ctx: &mut Context<Self>) -> Self::Result {
        async fn go_to_pos(self_pos: &mut Position, destination: &Position) {
            while self_pos.x != destination.x || self_pos.y != destination.y {
                self_pos.go_to(&destination);
                log::debug!("Current {:?} -> Destination {:?}", self_pos, destination);
                sleep(Duration::from_millis(750)).await;
            }
        }

        self.passenger_id = Some(msg.passenger_id);

        let central_driver = self.central_driver.clone();
        let current_pos = Arc::clone(&self.current_location);
        let self_addr = ctx.address().clone();

        self.trip_task = Some(
            ctx.spawn(
                async move {
                    let mut lock = current_pos.lock().await;

                    log::info!("[TRIP] Start trip for passenger {}", msg.passenger_id);

                    let _ = central_driver
                        .try_send(NotifyPositionToLeader {
                            driver_location: Position::infinity(),
                        })
                        .inspect(|_| log::debug!("Sent infinity!"))
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                        });

                    go_to_pos(&mut (*lock), &msg.passenger_location).await;

                    let _ = central_driver
                        .try_send(SendTripResponse {
                            passenger_id: msg.passenger_id,
                            status: TripStatus::Info,
                            detail: format!("I am at your door, come out!"),
                        })
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                        });

                    log::info!("[TRIP] Passenger {} picked up", msg.passenger_id);

                    go_to_pos(&mut (*lock), &msg.destination).await;

                    log::info!(
                        "[TRIP] Arrived at destination for passenger {}",
                        msg.passenger_id
                    );

                    let detail =
                        format!("We have arrived at our destination, we hope you enjoyed the trip");

                    let _ = central_driver
                        .try_send(SendTripResponse {
                            passenger_id: msg.passenger_id,
                            status: TripStatus::Success,
                            detail,
                        })
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                        });

                    let _ = central_driver
                        .try_send(CollectMoneyPassenger {
                            passenger_id: msg.passenger_id,
                        })
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                        });

                    self_addr.do_send(ClearPassenger {});
                }
                .into_actor(self),
            ),
        )
    }
}

#[derive(Message)]
#[rtype(result = "bool")]
pub struct CanHandleTrip {
    pub passenger_id: u32,
    pub passenger_location: Position,
    pub destination: Position,
    pub self_id: u32,
}

#[async_handler]
impl Handler<CanHandleTrip> for TripHandler {
    type Result = bool;

    async fn handle(&mut self, msg: CanHandleTrip, _ctx: &mut Context<Self>) -> Self::Result {
        let mut rng = rand::thread_rng();
        let response = self.passenger_id.is_none() && rng.gen_bool(TAKE_TRIP_PROBABILTY);

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

                    _ctx.notify(TripStart {
                        passenger_id: msg.passenger_id,
                        passenger_location: msg.passenger_location,
                        destination: msg.destination,
                    });

                    true
                }
                _ => false,
            }
        } else {
            false
        };

        res
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ClearPassenger {}
impl Handler<ClearPassenger> for TripHandler {
    type Result = ();

    fn handle(&mut self, _msg: ClearPassenger, ctx: &mut Context<Self>) -> Self::Result {
        if let None = self.passenger_id {
            return ();
        }

        self.passenger_id = None;

        if let Some(task) = self.trip_task {
            ctx.cancel_future(task);
            self.trip_task = None;
            log::warn!("What the hell!! The passenger jump out of the car!!")
        }

        log::info!("Now i'm ready for another trip!");
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ForceNotifyPosition {}
impl Handler<ForceNotifyPosition> for TripHandler {
    type Result = ();

    fn handle(&mut self, _msg: ForceNotifyPosition, ctx: &mut Context<Self>) -> Self::Result {
        let recipient = self.central_driver.clone();

        ctx.spawn({
            let position_lock: Arc<Mutex<Position>> = Arc::clone(&self.current_location);
            let test_env_var: Result<String, std::env::VarError> = std::env::var("TEST");

            async move {
                Self::notify_pos(&recipient, &position_lock, &test_env_var).await;
            }
            .into_actor(self)
        });
    }
}

use std::{sync::Arc, time::Duration};

use actix::{
    dev::ContextFutureSpawner, fut::wrap_future, Actor, Addr, AsyncContext, Context, Handler,
    Message, WrapFuture,
};
use common::utils::{json_parser::TripStatus, position::Position};
use rand::Rng;
use tokio::{sync::Mutex, time::sleep};

use crate::concu_driver::{central_driver::RedirectTripResponse, consts::TAKE_TRIP_PROBABILTY};

use super::{
    central_driver::{CentralDriver, NotifyPositionToLeader},
    consts::POSITION_NOTIFICATION_INTERVAL,
};

pub struct TripHandler {
    // Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    // Posicion actual del driver
    current_location: Arc<Mutex<Position>>,
    // Id del pasajero actual
    passenger_id: Option<u32>,
}

impl Actor for TripHandler {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let recipient = self.central_driver.clone();

        ctx.spawn({
            let position_lock = Arc::clone(&self.current_location);
            let test_env_var = std::env::var("TEST");
            async move {
                loop {
                    let mut lock = position_lock.lock().await;
                    // Simulate position change
                    match test_env_var {
                        Ok(_) => (),
                        Err(_) => (*lock).simulate(),
                    }

                    let _ = recipient
                        .try_send(NotifyPositionToLeader {
                            driver_location: *lock,
                        })
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                        });

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
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct TripStart {
    passenger_id: u32,
    passenger_location: Position,
    destination: Position,
    first_contact_driver: Option<u32>,
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

        wrap_future::<_, Self>(async move {
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

            log::info!("[TRIP] Passenger {} picked up", msg.passenger_id);

            go_to_pos(&mut (*lock), &msg.destination).await;

            log::info!(
                "[TRIP] Arrived at destination for passenger {}",
                msg.passenger_id
            );

            let detail =
                format!("We have arrived at our destination, we hope you enjoyed the trip");

            let _ = central_driver
                .try_send(RedirectTripResponse {
                    passenger_id: msg.passenger_id,
                    status: TripStatus::Success,
                    detail,
                    first_contact_driver: msg.first_contact_driver,
                })
                .inspect_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                });

            self_addr.do_send(ClearPassenger {});
            // NOTIFY PAYMENT
        })
        .spawn(ctx);
    }
}

#[derive(Message)]
#[rtype(result = "bool")]
pub struct CanHandleTrip {
    pub passenger_id: u32,
    pub passenger_location: Position,
    pub destination: Position,
    pub first_contact_driver: Option<u32>,
}

impl Handler<CanHandleTrip> for TripHandler {
    type Result = bool;

    fn handle(&mut self, msg: CanHandleTrip, ctx: &mut Context<Self>) -> Self::Result {
        let mut rng = rand::thread_rng();
        let response = if self.passenger_id.is_none() {
            rng.gen_bool(TAKE_TRIP_PROBABILTY)
        } else {
            false
        };

        if response {
            ctx.notify(TripStart {
                passenger_id: msg.passenger_id,
                passenger_location: msg.passenger_location,
                destination: msg.destination,
                first_contact_driver: msg.first_contact_driver,
            });
        }

        response
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ClearPassenger {}
impl Handler<ClearPassenger> for TripHandler {
    type Result = ();

    fn handle(&mut self, _msg: ClearPassenger, _ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Now i'm ready for another trip!");
        self.passenger_id = None;
    }
}

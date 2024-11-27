use std::{sync::Arc, time::Duration};

use actix::{
    dev::ContextFutureSpawner, fut::wrap_future, Actor, Addr, AsyncContext, Context, Handler,
    Message, WrapFuture,
};
use common::utils::position::Position;
use rand::Rng;
use tokio::{sync::Mutex, time::sleep};

use crate::concu_driver::consts::TAKE_TRIP_PROBABILTY;

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
            async move {
                loop {
                    let mut lock = position_lock.lock().await;
                    // Simulate position change
                    (*lock).simulate();

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
    pub fn new(central_driver: Addr<CentralDriver>) -> Self {
        Self {
            central_driver,
            current_location: Arc::new(Mutex::new(Position::random())),
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
}
impl Handler<TripStart> for TripHandler {
    type Result = ();

    fn handle(&mut self, msg: TripStart, ctx: &mut Context<Self>) -> Self::Result {
        self.passenger_id = Some(msg.passenger_id);

        let central_driver = self.central_driver.clone();
        let current_pos = Arc::clone(&self.current_location);

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

            while (*lock).x != msg.passenger_location.x || (*lock).y != msg.passenger_location.y {
                (*lock).go_to(&msg.passenger_location);
                log::debug!(
                    "Current {:?} -> Destination {:?}",
                    *lock,
                    msg.passenger_location
                );
                sleep(Duration::from_millis(750)).await;
            }

            log::info!("[TRIP] Passenger {} picked up", msg.passenger_id);

            while (*lock).x != msg.destination.x || (*lock).y != msg.destination.y {
                (*lock).go_to(&msg.destination);
                log::debug!("Current {:?} -> Destination {:?}", *lock, msg.destination);
                sleep(Duration::from_millis(750)).await;
            }

            log::info!("[TRIP] Trip for passenger {} completed", msg.passenger_id);

            // NOTIFY PAYMENT
        })
        .spawn(ctx);
    }
}

#[derive(Message)]
#[rtype(result = bool)]
pub struct CanHandleTrip {
    pub passenger_id: u32,
    pub passenger_location: Position,
    pub destination: Position,
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
            });
        }

        response
    }
}

use actix::{Actor, Addr, AsyncContext, Context, Message, WrapFuture};
use tokio::time::sleep;

use super::{
    central_driver::{CentralDriver, NotifyPositionToLeader},
    consts::POSITION_NOTIFICATION_INTERVAL,
    position::Position,
};

pub struct TripHandler {
    // Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    // Posicion actual del driver
    current_location: Position,
}

impl Actor for TripHandler {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let recipient = self.central_driver.clone();
        let mut initial_position = self.current_location.clone();

        ctx.spawn(
            async move {
                loop {
                    // Simulate position change
                    initial_position.simulate();

                    let _ = recipient
                        .try_send(NotifyPositionToLeader {
                            driver_location: initial_position.clone(),
                        })
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                        });

                    sleep(POSITION_NOTIFICATION_INTERVAL).await;
                }
            }
            .into_actor(self),
        );
    }
}

impl TripHandler {
    pub fn new(central_driver: Addr<CentralDriver>) -> Self {
        Self {
            central_driver,
            current_location: Position::random(),
        }
    }
}

#[derive(Message)]
#[rtype(result = String)]
struct TripStart {
    passenger_id: u32,
    passenger_location: Position,
    destination: Position,
}

use actix::{Actor, Addr, Context, Message};

use super::{
    central_driver::{self, CentralDriver},
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

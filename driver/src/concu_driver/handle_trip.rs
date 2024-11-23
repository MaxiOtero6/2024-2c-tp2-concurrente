use actix::{Actor, Addr, Context, Message};

use super::central_driver::CentralDriver;

pub struct TripHandler {
    // Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
}

impl Actor for TripHandler {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = String)]
struct TripStart {
    passenger_id: u32,
    passenger_location: (u32, u32),
    destination: (u32, u32),
}

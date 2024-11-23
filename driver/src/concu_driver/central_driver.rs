use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use actix::{Actor, Addr, Context, Message};

use super::{
    driver_connection::DriverConnection, handle_trip::TripHandler,
    passenger_connection::PassengerConnection, payment_connection::PaymentConnection,
};

pub struct CentralDriver {
    // Direccion del actor TripHandler
    trip_handler: Addr<TripHandler>,
    // Direccion del actor PassengerConnection
    connection_with_passenger: Addr<PassengerConnection>,
    // Direccion del actor PaymentConnection
    connection_with_payment: Addr<PaymentConnection>,
    // Direcciones de los drivers segun su id
    connection_with_drivers: HashMap<u32, Addr<DriverConnection>>, // 0...N
    // Posicion actual del driver
    current_location: (u32, u32),
    // Posiciones de los demas drivers segun su id,
    // cobra sentido si este driver es lider
    driver_positions: Arc<RwLock<HashMap<u32, (u32, u32)>>>,
}

impl Actor for CentralDriver {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "()")]
struct InfoPosition {
    driver_location: (u32, u32),
}

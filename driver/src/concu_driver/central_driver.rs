use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use actix::{Actor, Addr, AsyncContext, Context, Handler, Message};

use super::{
    driver_connection::DriverConnection, handle_trip::TripHandler,
    passenger_connection::PassengerConnection, payment_connection::PaymentConnection,
    position::Position,
};

pub struct CentralDriver {
    // Direccion del actor TripHandler
    trip_handler: Addr<TripHandler>,
    // Direccion del actor PassengerConnection
    connection_with_passenger: Option<Addr<PassengerConnection>>,
    // Direccion del actor PaymentConnection
    connection_with_payment: Option<Addr<PaymentConnection>>,
    // Direcciones de los drivers segun su id
    connection_with_drivers: HashMap<u32, Addr<DriverConnection>>, // 0...N
    // Posiciones de los demas drivers segun su id,
    // cobra sentido si este driver es lider
    driver_positions: Arc<RwLock<HashMap<u32, Position>>>,
    // Id del driver lider
    leader_id: Option<u32>,
    // Id del driver
    id: u32,
}

impl Actor for CentralDriver {
    type Context = Context<Self>;
}

impl CentralDriver {
    pub fn create_new(id: u32) -> Addr<Self> {
        CentralDriver::create(|ctx| Self {
            id,
            leader_id: None,
            driver_positions: Arc::new(RwLock::new(HashMap::new())),
            connection_with_drivers: HashMap::new(),
            trip_handler: TripHandler::new(ctx.address()).start(),
            connection_with_payment: None,
            connection_with_passenger: None,
        })
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct InfoPosition {
    driver_location: Position,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SetPaymentAddr {
    pub connection_with_payment: Addr<PaymentConnection>,
}

impl Handler<SetPaymentAddr> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: SetPaymentAddr, _ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Connecting with payments service");
        self.connection_with_payment = Some(msg.connection_with_payment);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SetPassengerAddr {
    pub connection_with_passenger: Addr<PassengerConnection>,
}

impl Handler<SetPassengerAddr> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: SetPassengerAddr, _ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Connecting with new passenger");
        self.connection_with_passenger = Some(msg.connection_with_passenger);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct InsertDriverConnection {
    pub id: u32,
    pub addr: Addr<DriverConnection>,
}

impl Handler<InsertDriverConnection> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: InsertDriverConnection, _ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Connecting with driver {}", msg.id);
        self.connection_with_drivers.insert(msg.id, msg.addr);
    }
}

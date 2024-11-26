use std::{
    os::unix::net::SocketAddr,
    sync::{Arc, Mutex},
};

use actix::{Actor, Addr, Context, Message};
use tokio::{io::WriteHalf, net::TcpStream};

use super::central_driver::CentralDriver;

pub struct PassengerConnection {
    // Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    // Stream para enviar al passenger
    passenger_write_stream: Arc<Mutex<WriteHalf<TcpStream>>>,
    // Direccion del stream del passenger
    passenger_addr: Option<SocketAddr>,
}

impl Actor for PassengerConnection {
    type Context = Context<Self>;
}

impl PassengerConnection {
    pub fn new(
        central_driver: Addr<CentralDriver>,
        write_stream: WriteHalf<TcpStream>,
        passenger_addr: SocketAddr,
    ) -> Self {
        Self {
            central_driver,
            passenger_write_stream: Arc::new(Mutex::new(write_stream)),
            passenger_addr: Some(passenger_addr),
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct TripStatus {
    log: String,
    msg_type: String,
}

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

#[derive(Message)]
#[rtype(result = "()")]
struct TripStatus {
    log: String,
    msg_type: String,
}

use std::{
    os::unix::net::SocketAddr,
    sync::{Arc, Mutex},
};

use actix::{Actor, Addr, Context, Message};
use tokio::{io::WriteHalf, net::TcpStream};

use super::central_driver::CentralDriver;

pub struct DriverConnection {
    // Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    // Stream para enviar al driver
    driver_write_stream: Arc<Mutex<WriteHalf<TcpStream>>>,
    // Direccion del stream del driver
    driver_addr: Option<SocketAddr>,
}

impl Actor for DriverConnection {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "u32")]
struct LeaderStatus {
    driver_ids: Vec<u32>,
    msg_type: String,
}

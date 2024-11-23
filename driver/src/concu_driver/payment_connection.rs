use std::{
    net::TcpStream,
    os::unix::net::SocketAddr,
    sync::{Arc, Mutex},
};

use actix::{Actor, Addr, Context, Message};
use tokio::io::WriteHalf;

use super::central_driver::CentralDriver;

pub struct PaymentConnection {
    // Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    // Stream para enviar al payment
    payment_write_stream: Arc<Mutex<WriteHalf<TcpStream>>>,
    // Direccion del stream del payment
    payment_addr: Option<SocketAddr>,
}

impl Actor for PaymentConnection {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "()")]
struct CollectMoneyPassenger {
    passenger_id: u32,
}

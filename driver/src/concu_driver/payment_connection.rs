use std::sync::{Arc, Mutex};

use actix::{Actor, Addr, AsyncContext, Context, Message, StreamHandler};
use tokio::{
    io::{split, AsyncBufReadExt, BufReader, WriteHalf},
    net::TcpStream,
};
use tokio_stream::wrappers::LinesStream;

use super::central_driver::CentralDriver;

use common::utils::consts::{HOST, PAYMENT_PORT};
pub struct PaymentConnection {
    // Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    // Stream para enviar al payment
    payment_write_stream: Arc<Mutex<WriteHalf<TcpStream>>>,
}

impl Actor for PaymentConnection {
    type Context = Context<Self>;
}

impl PaymentConnection {
    fn new(central_driver: Addr<CentralDriver>, write_stream: WriteHalf<TcpStream>) -> Self {
        Self {
            central_driver,
            payment_write_stream: Arc::new(Mutex::new(write_stream)),
        }
    }

    pub async fn connect(
        central_driver: Addr<CentralDriver>,
    ) -> Result<Addr<PaymentConnection>, String> {
        log::debug!("Trying to connect with payments service");

        let addr = format!("{}:{}", HOST, PAYMENT_PORT);

        let socket = TcpStream::connect(addr.clone()).await.map_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            e.to_string()
        })?;

        let (r, w) = split(socket);

        Ok(PaymentConnection::create(|ctx| {
            ctx.add_stream(LinesStream::new(BufReader::new(r).lines()));
            PaymentConnection::new(central_driver, w)
        }))
    }
}

impl StreamHandler<Result<String, std::io::Error>> for PaymentConnection {
    fn handle(&mut self, msg: Result<String, std::io::Error>, ctx: &mut Self::Context) {}
}

#[derive(Message)]
#[rtype(result = "()")]
struct CollectMoneyPassenger {
    passenger_id: u32,
}

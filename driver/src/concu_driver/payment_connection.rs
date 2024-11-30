use std::sync::Arc;

use actix::{
    dev::ContextFutureSpawner, fut::wrap_future, Actor, ActorContext, Addr, AsyncContext, Context,
    Handler, Message, StreamHandler,
};
use tokio::{
    io::{split, AsyncBufReadExt, AsyncWriteExt, BufReader, WriteHalf},
    net::TcpStream,
    sync::Mutex,
};
use tokio_stream::wrappers::LinesStream;

use crate::concu_driver::central_driver::CheckPaymentResponse;

use super::central_driver::CentralDriver;

use common::utils::{
    consts::{HOST, PAYMENT_PORT},
    json_parser::PaymentResponses,
};
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
    fn handle(&mut self, msg: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        if let Ok(data) = msg {
            // log::debug!("recv {}", data);

            let _ = ctx.address().try_send(RecvAll { data }).inspect_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
            });
        }
    }

    fn finished(&mut self, ctx: &mut Self::Context) {
        ctx.stop();
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendAll {
    pub data: String,
}

impl Handler<SendAll> for PaymentConnection {
    type Result = ();

    fn handle(&mut self, msg: SendAll, ctx: &mut Context<Self>) -> Self::Result {
        let message = msg.data + "\n";

        let w = self.payment_write_stream.clone();
        wrap_future::<_, Self>(async move {
            let mut writer = w.lock().await;

            let _ = writer.write_all(message.as_bytes()).await.inspect_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
            });

            let _ = writer.flush().await.inspect_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
            });

            // log::debug!("sent {}", message);
        })
        .spawn(ctx);
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct RecvAll {
    pub data: String,
}

impl Handler<RecvAll> for PaymentConnection {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: RecvAll, _ctx: &mut Context<Self>) -> Self::Result {
        let data = serde_json::from_str(&msg.data).map_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            e.to_string()
        })?;

        match data {
            PaymentResponses::CollectPayment {
                passenger_id,
                response,
            } => {
                let _ = self
                    .central_driver
                    .try_send(CheckPaymentResponse {
                        passenger_id,
                        response,
                    })
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    });
            }
            PaymentResponses::AuthPayment {
                passenger_id: _,
                response: _,
            } => log::error!("Why i'm receiving a payment auth response?"),
        }

        Ok(())
    }
}

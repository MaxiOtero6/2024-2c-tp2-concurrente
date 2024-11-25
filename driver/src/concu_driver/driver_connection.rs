use std::{
    error::Error,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use actix::{
    dev::ContextFutureSpawner, fut::wrap_future, Actor, ActorContext, Addr, AsyncContext, Context,
    Handler, Message, StreamHandler,
};
use tokio::{
    io::{AsyncWriteExt, WriteHalf},
    net::TcpStream,
};

use super::{
    central_driver::{
        Alive, CentralDriver, Coordinator, Election, InsertDriverConnection, StartElection,
    },
    json_parser::DriverMessages,
};

pub struct DriverConnection {
    // Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    // Stream para enviar al driver
    driver_write_stream: Arc<Mutex<WriteHalf<TcpStream>>>,
    // Direccion del stream del driver
    driver_addr: Option<SocketAddr>,
    // ID del driver
    id: u32,
}

impl DriverConnection {
    pub fn new(
        id: u32,
        self_driver_addr: Addr<CentralDriver>,
        wstream: Arc<Mutex<WriteHalf<TcpStream>>>,
        driver_addr: Option<SocketAddr>,
    ) -> Self {
        DriverConnection {
            id,
            central_driver: self_driver_addr,
            driver_write_stream: wstream,
            driver_addr,
        }
    }
}

impl StreamHandler<Result<String, std::io::Error>> for DriverConnection {
    fn handle(&mut self, msg: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        if let Ok(data) = msg {
            log::debug!("recv {}", data);

            let _ = ctx
                .address()
                .try_send(RecvAll { data })
                .inspect_err(|e| log::error!("{}", e.to_string()));
        }
    }

    fn finished(&mut self, ctx: &mut Self::Context) {
        // Election
        self.central_driver.do_send(StartElection {});

        ctx.stop();
    }
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

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendAll {
    pub data: String,
}

impl Handler<SendAll> for DriverConnection {
    type Result = ();

    fn handle(&mut self, msg: SendAll, ctx: &mut Context<Self>) -> Self::Result {
        let message = msg.data + "\n";

        let w = self.driver_write_stream.clone();
        wrap_future::<_, Self>(async move {
            if let Ok(mut writer) = w.lock() {
                let _ = writer
                    .write_all(message.as_bytes())
                    .await
                    .inspect_err(|e| log::error!("{}", e.to_string()));
                let _ = writer
                    .flush()
                    .await
                    .inspect_err(|e| log::error!("{}", e.to_string()))
                    .inspect(|_| log::debug!("sent {}", message));
            }
        })
        .spawn(ctx);
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct RecvAll {
    pub data: String,
}

impl Handler<RecvAll> for DriverConnection {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: RecvAll, ctx: &mut Context<Self>) -> Self::Result {
        let data = serde_json::from_str(&msg.data).map_err(|e| e.to_string())?;

        match data {
            DriverMessages::RequestDriverId {} => {
                let data = DriverMessages::ResponseDriverId { id: self.id };
                let parsed_json = serde_json::to_string(&data).map_err(|e| e.to_string())?;
                ctx.address()
                    .try_send(SendAll { data: parsed_json })
                    .map_err(|e| e.to_string())?;
            }
            DriverMessages::ResponseDriverId { id } => {
                self.central_driver
                    .try_send(InsertDriverConnection {
                        id,
                        addr: ctx.address(),
                    })
                    .map_err(|e| e.to_string())?;
            }
            DriverMessages::Election { sender_id } => {
                self.central_driver
                    .try_send(Election { sender_id })
                    .map_err(|e| e.to_string())?;
            }
            DriverMessages::Alive { responder_id } => {
                self.central_driver
                    .try_send(Alive { responder_id })
                    .map_err(|e| e.to_string())?;
            }
            DriverMessages::Coordinator { leader_id } => {
                self.central_driver
                    .try_send(Coordinator { leader_id })
                    .map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }
}

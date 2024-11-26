use std::{
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

use crate::concu_driver::central_driver::RemoveDriverConnection;

use super::{
    central_driver::{
        Alive, CentralDriver, Coordinator, Election, SetDriverPosition, StartElection,
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
    // ID de este driver
    id: u32,
    // ID del driver
    driver_id: u32,
}

impl DriverConnection {
    pub fn new(
        id: u32,
        self_driver_addr: Addr<CentralDriver>,
        wstream: Arc<Mutex<WriteHalf<TcpStream>>>,
        driver_addr: Option<SocketAddr>,
        driver_id: u32,
    ) -> Self {
        DriverConnection {
            id,
            central_driver: self_driver_addr,
            driver_write_stream: wstream,
            driver_addr,
            driver_id,
        }
    }
}

impl StreamHandler<Result<String, std::io::Error>> for DriverConnection {
    fn handle(&mut self, msg: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        if let Ok(data) = msg {
            log::debug!("recv {}", data);

            let _ = ctx.address().try_send(RecvAll { data }).inspect_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
            });
        }
    }

    fn finished(&mut self, ctx: &mut Self::Context) {
        // if let Some(did) = self.driver_id {
        log::warn!("Broken pipe with driver {}", self.driver_id);
        self.central_driver
            .do_send(RemoveDriverConnection { id: self.driver_id });
        // }
        // Election
        self.central_driver.do_send(StartElection {});

        ctx.stop();
    }
}

impl Actor for DriverConnection {
    type Context = Context<Self>;
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
                let _ = writer.write_all(message.as_bytes()).await.inspect_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                });
                let _ = writer
                    .flush()
                    .await
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                    })
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

    fn handle(&mut self, msg: RecvAll, _ctx: &mut Context<Self>) -> Self::Result {
        let data = serde_json::from_str(&msg.data).map_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            e.to_string()
        })?;

        match data {
            DriverMessages::Election { sender_id } => {
                self.central_driver
                    .try_send(Election { sender_id })
                    .map_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        e.to_string()
                    })?;
            }
            DriverMessages::Alive { responder_id } => {
                self.central_driver
                    .try_send(Alive { responder_id })
                    .map_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        e.to_string()
                    })?;
            }
            DriverMessages::Coordinator { leader_id } => {
                self.central_driver
                    .try_send(Coordinator { leader_id })
                    .map_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        e.to_string()
                    })?;
            }
            DriverMessages::NotifyPosition {
                driver_id,
                driver_position,
            } => {
                self.central_driver
                    .try_send(SetDriverPosition {
                        driver_id,
                        driver_position,
                    })
                    .map_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        e.to_string()
                    })?;
            }
        }

        Ok(())
    }
}

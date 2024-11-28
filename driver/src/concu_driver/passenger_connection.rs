use std::sync::Arc;

use actix::{
    dev::ContextFutureSpawner, fut::wrap_future, Actor, ActorContext, Addr, AsyncContext, Context,
    Handler, Message, StreamHandler,
};
use common::utils::json_parser::TripMessages;
use tokio::{
    io::{AsyncWriteExt, WriteHalf},
    net::TcpStream,
    sync::Mutex,
};

use crate::concu_driver::central_driver::RemovePassengerConnection;

use super::central_driver::{CentralDriver, RedirectNewTrip};

pub struct PassengerConnection {
    // Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    // Stream para enviar al passenger
    passenger_write_stream: Arc<Mutex<WriteHalf<TcpStream>>>,
    // ID del pasajero
    passenger_id: u32,
}

impl PassengerConnection {
    pub fn new(
        central_driver: Addr<CentralDriver>,
        write_stream: WriteHalf<TcpStream>,
        passenger_id: u32,
    ) -> Self {
        Self {
            central_driver,
            passenger_write_stream: Arc::new(Mutex::new(write_stream)),
            passenger_id,
        }
    }
}

impl Actor for PassengerConnection {
    type Context = Context<Self>;
}

impl StreamHandler<Result<String, std::io::Error>> for PassengerConnection {
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
        log::warn!("Broken pipe with passenger {}", self.passenger_id);
        self.central_driver.do_send(RemovePassengerConnection {
            id: self.passenger_id,
        });
        // }

        ctx.stop();
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendAll {
    pub data: String,
}

impl Handler<SendAll> for PassengerConnection {
    type Result = ();

    fn handle(&mut self, msg: SendAll, ctx: &mut Context<Self>) -> Self::Result {
        let message = msg.data + "\n";

        let w = self.passenger_write_stream.clone();
        wrap_future::<_, Self>(async move {
            let mut writer = w.lock().await;

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
        })
        .spawn(ctx);
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct RecvAll {
    pub data: String,
}

impl Handler<RecvAll> for PassengerConnection {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: RecvAll, _ctx: &mut Context<Self>) -> Self::Result {
        let data = serde_json::from_str(&msg.data).map_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            e.to_string()
        })?;

        match data {
            TripMessages::TripRequest {
                source,
                destination,
            } => self
                .central_driver
                .try_send(RedirectNewTrip {
                    passenger_id: self.passenger_id,
                    source,
                    destination,
                    first_contact_driver: None,
                })
                .map_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    e.to_string()
                })?,

            TripMessages::TripResponse {
                status: _,
                detail: _,
            } => log::error!("Why i'm receiving a trip response?"),
        }

        Ok(())
    }
}

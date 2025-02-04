use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, StreamHandler};
use actix_async_handler::async_handler;
use tokio::{
    io::{AsyncWriteExt, WriteHalf},
    net::TcpStream,
};

use crate::concu_driver::central_driver::RemoveDriverConnection;

use super::{
    central_driver::{
        Alive, CanHandleTrip, CanHandleTripACK, CentralDriver, Coordinator, Election,
        RedirectNewTrip, SetDriverPosition, StartElection,
    },
    json_parser::DriverMessages,
};

pub struct DriverConnection {
    /// Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    /// Stream para enviar al driver
    driver_write_stream: Option<WriteHalf<TcpStream>>,
    /// ID del driver
    driver_id: u32,
}

impl DriverConnection {
    /// Crea una nueva conexión con un driver con:
    /// - La dirección del actor `CentralDriver`
    /// - El stream de escritura
    /// - ID del driver.
    /// - Retorna la conexión con el driver.
    pub fn new(
        self_driver_addr: Addr<CentralDriver>,
        wstream: WriteHalf<TcpStream>,
        driver_id: u32,
    ) -> Self {
        DriverConnection {
            central_driver: self_driver_addr,
            driver_write_stream: Some(wstream),
            driver_id,
        }
    }
}

impl StreamHandler<Result<String, std::io::Error>> for DriverConnection {
    /// Maneja los mensajes recibidos desde los drivers.
    /// Verifica si el mensaje es un mensaje válido y en caso de serlo envía un mensaje a si mismo "RecvAll" con el mensaje recibido.
    fn handle(&mut self, msg: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        if let Ok(data) = msg {
            // log::debug!("recv {}", data);

            let _ = ctx.address().try_send(RecvAll { data }).inspect_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
            });
        }
    }

    /// Maneja la finalización del flujo asociado al actor `DriverConnection`.
    /// Envia un mensaje al actor `CentralDriver` para eliminar la conexión con el driver.
    ///
    /// Inicia una elección(porque puede ser la situación en la que el driver que se desconectó era el líder).
    fn finished(&mut self, _ctx: &mut Self::Context) {
        // if let Some(did) = self.driver_id {
        log::warn!("Broken pipe with driver {}", self.driver_id);
        self.central_driver
            .do_send(RemoveDriverConnection { id: self.driver_id });
        // }
        // Election
        self.central_driver.do_send(StartElection {});

        // ctx.stop();
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

#[async_handler]
impl Handler<SendAll> for DriverConnection {
    type Result = ();

    /// Maneja el envío de mensajes a los drivers.
    ///
    /// Genera una tarea asincrónica en donde lockea el stream de escritura y escribe el mensaje en el stream.

    async fn handle(&mut self, msg: SendAll, _ctx: &mut Context<Self>) -> Self::Result {
        let message = msg.data + "\n";

        let w = self.driver_write_stream.take();

        if let Some(mut wstream) = w {
            let r = async move {
                let _ = wstream
                    .write_all(message.as_bytes())
                    .await
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                    });

                let _ = wstream.flush().await.inspect_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                });

                wstream
            }
            .await;

            // log::debug!("sent {}", message)

            self.driver_write_stream = Some(r);
        } else {
            log::error!(
                "{}:{}, {}",
                std::file!(),
                std::line!(),
                "Must wait until the previous SendAll message is finished"
            );
        }
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct RecvAll {
    pub data: String,
}

impl Handler<RecvAll> for DriverConnection {
    type Result = Result<(), String>;

    /// Maneja los mensajes recibidos desde los drivers.
    /// Parsea el mensaje recibido segun el tipo de mensaje y envía un mensaje al actor `CentralDriver` con la respuesta o acción correspondiente.
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
            DriverMessages::CanHandleTrip {
                passenger_id,
                passenger_location,
                destination,
                driver_id,
            } => {
                self.central_driver
                    .try_send(CanHandleTrip {
                        passenger_id,
                        source: passenger_location,
                        destination,
                        driver_id,
                    })
                    .map_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        e.to_string()
                    })?;
            }
            DriverMessages::CanHandleTripACK {
                response,
                passenger_id,
                driver_id,
            } => {
                self.central_driver
                    .try_send(CanHandleTripACK {
                        response,
                        passenger_id,
                        driver_id,
                    })
                    .map_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        e.to_string()
                    })?;
            }
            DriverMessages::TripRequest {
                passenger_id,
                passenger_location,
                destination,
            } => self
                .central_driver
                .try_send(RedirectNewTrip {
                    passenger_id,
                    source: passenger_location,
                    destination,
                })
                .map_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    e.to_string()
                })?,
        }

        Ok(())
    }
}

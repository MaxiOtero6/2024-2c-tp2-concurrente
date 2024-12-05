use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, StreamHandler};
use actix_async_handler::async_handler;
use common::utils::{
    consts::{HOST, MIN_PASSENGER_PORT},
    json_parser::TripMessages,
};
use tokio::{
    io::{split, AsyncBufReadExt, AsyncWriteExt, BufReader, WriteHalf},
    net::TcpStream,
};
use tokio_stream::wrappers::LinesStream;

use crate::concu_driver::central_driver::RemovePassengerConnection;

use super::central_driver::{CentralDriver, RedirectNewTrip};

pub struct PassengerConnection {
    /// Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    /// Stream para enviar al passenger
    passenger_write_stream: Option<WriteHalf<TcpStream>>,
    /// ID del pasajero
    passenger_id: u32,
}

impl PassengerConnection {
    /// Crea una nueva conexión con un pasajero con:
    /// - La dirección del actor `CentralDriver`
    /// - El stream de escritura
    /// - ID del pasajero.
    pub fn new(
        central_driver: Addr<CentralDriver>,
        write_stream: WriteHalf<TcpStream>,
        passenger_id: u32,
    ) -> Self {
        Self {
            central_driver,
            passenger_write_stream: Some(write_stream),
            passenger_id,
        }
    }

    /// Establece una conexión con un pasajero.
    /// Se conecta al puerto `MIN_PASSENGER_PORT + passenger_id` en el host `HOST`.
    /// Hace el split del socket TCP en un lector y un escritor.
    /// Crea un nuevo actor `PassengerConnection` que:
    /// - Agrega un flujo de líneas de texto (provenientes del lector) al contexto del actor.
    /// - Retorna la dirección del actor `PassengerConnection` si la conexión es exitosa.
    /// - Retorna un mensaje de error si ocurre un problema durante la conexión.
    pub async fn connect(
        central_driver: Addr<CentralDriver>,
        passenger_id: u32,
    ) -> Result<Addr<Self>, String> {
        // log::debug!("Trying to connect with passenger {}", passenger_id);

        let addr = format!("{}:{}", HOST, MIN_PASSENGER_PORT + passenger_id);

        match TcpStream::connect(addr).await {
            Ok(socket) => {
                let (r, w) = split(socket);

                let passenger_conn = PassengerConnection::create(|ctx| {
                    ctx.add_stream(LinesStream::new(BufReader::new(r).lines()));
                    Self::new(central_driver, w, passenger_id)
                });

                Ok(passenger_conn)
            }
            Err(e) => {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                Err(e.to_string())
            }
        }
    }
}

impl Actor for PassengerConnection {
    type Context = Context<Self>;
}

impl StreamHandler<Result<String, std::io::Error>> for PassengerConnection {
    /// Maneja los mensajes recibidos por el stream de lectura del pasajero.
    /// Si el mensaje recibido es válido, lo envía al actor `RecvAll`.
    fn handle(&mut self, msg: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        if let Ok(data) = msg {
            // log::debug!("recv {}", data);

            let _ = ctx.address().try_send(RecvAll { data }).inspect_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
            });
        }
    }

    /// Maneja la finalización del stream de lectura del pasajero.
    /// Si el stream se cierra, se envía un mensaje al actor `CentralDriver` para eliminar la conexión con el pasajero.
    /// Luego, se detiene el actor.
    fn finished(&mut self, _ctx: &mut Self::Context) {
        // if let Some(did) = self.driver_id {
        log::warn!("Broken pipe with passenger {}", self.passenger_id);
        self.central_driver.do_send(RemovePassengerConnection {
            id: self.passenger_id,
        });
        // }

        // ctx.stop();
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendAll {
    pub data: String,
}

#[async_handler]
impl Handler<SendAll> for PassengerConnection {
    type Result = ();

    /// Envía un mensaje al pasajero.
    /// Lanza una tarea asincrónica en donde lockea el stream de escritura y escribe el mensaje en el stream.
    async fn handle(&mut self, msg: SendAll, _ctx: &mut Context<Self>) -> Self::Result {
        let message = msg.data + "\n";

        let w = self.passenger_write_stream.take();

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

            self.passenger_write_stream = Some(r);
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

impl Handler<RecvAll> for PassengerConnection {
    type Result = Result<(), String>;

    /// Maneja los mensajes recibidos desde el pasajero.
    /// Parsea el mensaje recibido y envía un mensaje:
    /// Si el mensaje es de tipo `TripRequest` envía un mensaje al `CentralDriver` con la respuesta.
    /// Si el mensaje no es de tipo `TripRequest` loggea un error.
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
                })
                .map_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    e.to_string()
                })?,

            _ => log::error!("Why i'm receiving a this type of message {:?}", data),
        }

        Ok(())
    }
}

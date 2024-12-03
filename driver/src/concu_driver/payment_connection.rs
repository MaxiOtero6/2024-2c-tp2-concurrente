use std::sync::Arc;

use actix::{
    dev::ContextFutureSpawner, fut::wrap_future, Actor, Addr, AsyncContext, Context, Handler,
    Message, StreamHandler,
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
    /// Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    /// Stream para enviar al payment
    payment_write_stream: Arc<Mutex<WriteHalf<TcpStream>>>,
}

impl Actor for PaymentConnection {
    type Context = Context<Self>;
}

/// Implementa la creacion de un PaymentConnection
impl PaymentConnection {
    fn new(central_driver: Addr<CentralDriver>, write_stream: WriteHalf<TcpStream>) -> Self {
        Self {
            central_driver,
            payment_write_stream: Arc::new(Mutex::new(write_stream)),
        }
    }

    /// Establece una conexión con el servicio de pagos.
    ///
    /// # Parámetros
    /// - `central_driver`: Un `Addr<CentralDriver>` que representa un actor responsable
    ///   de manejar la lógica centralizada de la aplicación.
    ///
    /// # Retorno
    /// Retorna un `Result` que contiene:
    /// - `Addr<PaymentConnection>`: La dirección del actor que representa la conexión de pagos, si es exitosa.
    /// - `String`: Un mensaje de error si ocurre un problema durante la conexión.
    ///
    /// # Descripción
    /// 1. Registra un mensaje de depuración indicando que se intentará conectar al servicio de pagos.
    /// 2. Construye la dirección del servicio remoto usando las constantes `HOST` y `PAYMENT_PORT`.
    /// 3. Intenta establecer una conexión TCP con el servicio de pagos:
    ///    - Si falla, registra el error y retorna el error como un `String`.
    /// 4. Si la conexión es exitosa, divide el socket TCP en un lector (`r`) y un escritor (`w`).
    /// 5. Crea un nuevo actor `PaymentConnection` que:
    ///    - Agrega un flujo de líneas de texto (provenientes del lector) al contexto del actor.
    ///    - Inicializa el actor con una referencia al `central_driver` clonado y el escritor (`w`).
    /// 6. Retorna la dirección del actor `PaymentConnection` en caso de éxito.

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
            PaymentConnection::new(central_driver.clone(), w)
        }))
    }
}




impl StreamHandler<Result<String, std::io::Error>> for PaymentConnection {

    /// Maneja los mensajes recibidos desde un flujo asíncrono asociado al actor `PaymentConnection`.
    ///
    /// Este método es parte de la implementación del trait `StreamHandler`, que permite que
    /// `PaymentConnection` procese datos de manera concurrente mientras llegan desde un flujo.
    ///
    /// # Parámetros
    /// - `msg`: Un `Result` que contiene:
    ///   - `Ok(String)`: Datos válidos recibidos desde el flujo.
    ///   - `Err(std::io::Error)`: Un error ocurrido durante la lectura del flujo.
    /// - `ctx`: El contexto del actor (`Self::Context`), usado para interactuar con otros actores
    ///   o enviar mensajes adicionales.
    ///
    /// # Comportamiento
    /// - Si el mensaje contiene datos válidos (`Ok(String)`):
    ///   - Extrae el contenido del mensaje (`data`).
    ///   - Envía un mensaje del tipo `RecvAll` al propio actor utilizando `ctx.notify`.
    /// - Si el mensaje contiene un error (`Err`):
    ///   - El error se ignora silenciosamente, sin afectar el flujo del programa.

    fn handle(&mut self, msg: Result<String, std::io::Error>, ctx: &mut Self::Context) {
        if let Ok(data) = msg {
            // log::debug!("recv {}", data);

            ctx.notify(RecvAll { data });
        }
    }


    /// Maneja la finalización del flujo asociado al actor `PaymentConnection`.
    fn finished(&mut self, _ctx: &mut Self::Context) {}
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendAll {
    pub data: String,
}

impl Handler<SendAll> for PaymentConnection {
    type Result = ();


    /// Maneja el envío de mensajes al servicio de pagos.
    /// Clona el stream de escritura y envía el mensaje al servicio.
    /// Lanza una tarea asincrónica para enviar el mensaje donde lockea el stream de escritura
    /// y escribe el mensaje en el stream.
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

    /// Maneja los mensajes recibidos desde el servicio de pagos.
    /// Parsea el mensaje recibido y envía un mensaje al actor `CentralDriver` con la respuesta.
    /// Si el mensaje es de tipo `CollectPayment` envía un mensaje al `CentralDriver` con la respuesta.
    /// Si el mensaje es de tipo `AuthPayment` loggea un error.
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

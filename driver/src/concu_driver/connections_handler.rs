use std::time::Duration;

use actix::{Actor, Addr, AsyncContext};
use common::utils::{
    consts::{HOST, MAX_DRIVER_PORT, MIN_DRIVER_PORT},
    json_parser::TripMessages,
};
use tokio::{
    io::{split, AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
    time::timeout,
};
use tokio_stream::wrappers::LinesStream;

use crate::concu_driver::{central_driver::StartElection, json_parser::CommonMessages};

use super::{
    central_driver::{CentralDriver, InsertDriverConnection, RedirectNewTrip},
    driver_connection::DriverConnection,
};

pub struct DriverConnectionsHandler;

impl DriverConnectionsHandler {
    /// Corre el setUp del driver a la hora de crearse
    pub fn run(
        id: u32,
        central_driver_addr: Addr<CentralDriver>,
    ) -> JoinHandle<Result<(), String>> {
        actix::spawn(async move { Self::setup(&central_driver_addr, id).await })
    }

    /// Conecta a todos los drivers
    ///
    /// Mientras el driver_id sea menor o igual al maximo de drivers, se conecta a cada uno
    async fn connect_all_drivers(
        self_id: u32,
        central_driver_addr: &Addr<CentralDriver>,
    ) -> Result<(), String> {
        let mut driver_id = 0;

        let max_id = MAX_DRIVER_PORT - MIN_DRIVER_PORT;

        while driver_id <= max_id {
            let addr = format!("{}:{}", HOST, MIN_DRIVER_PORT + driver_id);

            if let Ok(mut socket) = TcpStream::connect(addr.clone()).await {
                let request = serde_json::to_string(&CommonMessages::Identification {
                    id: self_id,
                    type_: 'D',
                })
                .map_err(|e| {
                    format!("Error connecting with {}, reason: {}", addr, e.to_string())
                })?;

                socket
                    .write_all((request + "\n").as_bytes())
                    .await
                    .map_err(|e| {
                        format!("Error connecting with {}, reason: {}", addr, e.to_string())
                    })?;

                let (r, w) = split(socket);

                Self::connect_with_driver(central_driver_addr, r, w, driver_id).await?;
            }

            driver_id += 1;
        }

        Ok(())
    }

    /// Setea el driver
    /// - Se conecta con todos los drivers
    /// - Comienza una nueva elección
    /// - Se pone a escuchar por nuevas conexiones
    ///
    /// Puede tener dos posibles conexiones:
    ///  - Con un driver: Se crea un nuevo actor DriverConnection y se le pasa un stream de lineas para que escuche los mensajes
    /// - Con un pasajero: Se lee del stream para ver si recibio algun mensaje y lo handlea como debe
    async fn setup(central_driver_addr: &Addr<CentralDriver>, id: u32) -> Result<(), String> {
        Self::connect_all_drivers(id, central_driver_addr).await?;

        // raise election
        central_driver_addr
            .try_send(StartElection {})
            .map_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                e.to_string()
            })?;

        let self_addr = format!("{}:{}", HOST, MIN_DRIVER_PORT + id);

        log::info!("My addr is {}", self_addr);

        let listener = TcpListener::bind(self_addr).await.map_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            e.to_string()
        })?;

        log::info!("Listening to new connections!");

        loop {
            let (socket, addr) = listener.accept().await.map_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                e.to_string()
            })?;

            log::debug!("Connection accepted from {}", addr);

            let (mut r, w) = split(socket);

            let mut reader = BufReader::new(&mut r);

            let mut str_response = String::new();

            reader.read_line(&mut str_response).await.map_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                e.to_string()
            })?;

            if str_response.is_empty() {
                log::error!("Error receiving identification");
            }

            let response: CommonMessages = serde_json::from_str(&str_response).map_err(|e| {
                log::error!(
                    "{}:{}, {}, str: {}, len: {}",
                    std::file!(),
                    std::line!(),
                    e.to_string(),
                    str_response,
                    str_response.len()
                );
                e.to_string()
            })?;

            match response {
                CommonMessages::Identification { id, type_ } => match type_ {
                    'D' => {
                        let _ = Self::connect_with_driver(central_driver_addr, r, w, id).await;
                    }
                    'P' => {
                        let _ =
                            Self::handle_passenger_connection(central_driver_addr, w, id, reader)
                                .await;
                    }
                    _ => (),
                },
            }
        }
    }

    /// Creas el Actor DriverConnection y le agregas un stream de lineas para que escuche los mensajes
    async fn connect_with_driver(
        central_driver_addr: &Addr<CentralDriver>,
        r: ReadHalf<TcpStream>,
        w: WriteHalf<TcpStream>,
        driver_id: u32,
    ) -> Result<(), String> {
        let driver_conn = DriverConnection::create(|ctx| {
            ctx.add_stream(LinesStream::new(BufReader::new(r).lines()));
            DriverConnection::new(central_driver_addr.clone(), w, driver_id)
        });

        central_driver_addr
            .try_send(InsertDriverConnection {
                id: driver_id,
                addr: driver_conn,
            })
            .map_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                e.to_string()
            })
    }

    /// Conecta con un pasajero
    /// Lee del strem para ver si recibio algun mensaje.
    /// - En el caso de recirlo lo parsea y espera otro mensaje 'Listening', luego le envia el mensaje RedirectTrip al central_driver
    /// - En el caso de no recibir ambos, devuelve un error
    ///
    /// Luego se envia un mensaje de confirmacion al pasajero de que su viaje esta siendo procesado
    ///
    async fn handle_passenger_connection(
        central_driver_addr: &Addr<CentralDriver>,
        mut w: WriteHalf<TcpStream>,
        passenger_id: u32,
        mut reader: BufReader<&mut ReadHalf<TcpStream>>,
    ) -> Result<(), String> {
        let mut str_response = String::new();

        reader.read_line(&mut str_response).await.map_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            e.to_string()
        })?;

        if str_response.is_empty() {
            log::error!("Error receiving trip request");
            return Err("Error receiving trip request".into());
        }

        let response: TripMessages = serde_json::from_str(&str_response).map_err(|e| {
            log::error!(
                "{}:{}, {}, str: {}, len: {}",
                std::file!(),
                std::line!(),
                e.to_string(),
                str_response,
                str_response.len()
            );
            e.to_string()
        })?;

        let trip_data = match response {
            TripMessages::TripRequest {
                source,
                destination,
            } => (source, destination),
            _ => {
                log::error!("{}:{}, TripRequest expected", std::file!(), std::line!());
                return Err("TripRequest expected".into());
            }
        };

        let (source, destination) = trip_data;

        let mut listen_message = String::new();

        let reader_ret = timeout(
            Duration::from_millis(500),
            reader.read_line(&mut listen_message),
        )
        .await
        .map_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            e.to_string()
        })?;

        if reader_ret.is_err() || listen_message.is_empty() {
            log::error!("Error receiving listening notification");
            return Err("Error receiving listening notification".into());
        }

        let listen_response: TripMessages = serde_json::from_str(&listen_message).map_err(|e| {
            log::error!(
                "{}:{}, {}, str: {}, len: {}",
                std::file!(),
                std::line!(),
                e.to_string(),
                listen_message,
                listen_message.len()
            );
            e.to_string()
        })?;

        match listen_response {
            TripMessages::Listening {} => central_driver_addr
                .try_send(RedirectNewTrip {
                    passenger_id,
                    source,
                    destination,
                })
                .map_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    e.to_string()
                })?,
            _ => {
                log::error!(
                    "{}:{}, Listening notification expected",
                    std::file!(),
                    std::line!()
                );
                return Err("Listening notification expected".into());
            }
        };

        let parsed_data = serde_json::to_string(&TripMessages::TripResponse {
            status: common::utils::json_parser::TripStatus::RequestDelivered,
            detail: "Your request has been delivered, a driver will pick you up soon".to_string(),
        })
        .inspect_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        });

        if let Ok(data) = parsed_data {
            w.write_all((data + "\n").as_bytes()).await.map_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                e.to_string()
            })?;

            w.flush().await.map_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                e.to_string()
            })?;
        }

        Ok(())
    }
}

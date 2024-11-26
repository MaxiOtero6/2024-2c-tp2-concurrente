use core::str;
use std::sync::{Arc, Mutex};

use actix::{Actor, Addr, AsyncContext};
use tokio::{
    io::{split, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_stream::wrappers::LinesStream;

use crate::concu_driver::{central_driver::StartElection, json_parser::CommonMessages};

use super::{
    central_driver::{CentralDriver, InsertDriverConnection},
    consts::{MAX_DRIVER_PORT, MIN_DRIVER_PORT},
    driver_connection::DriverConnection,
    utils::get_driver_address_by_id,
};

pub struct DriverConnectionsHandler;

impl DriverConnectionsHandler {
    pub fn run(
        id: u32,
        central_driver_addr: Addr<CentralDriver>,
    ) -> JoinHandle<Result<(), String>> {
        actix::spawn(async move { Self::setup(&central_driver_addr, id).await })
    }

    async fn connect_all_drivers(
        self_id: u32,
        central_driver_addr: &Addr<CentralDriver>,
    ) -> Result<(), String> {
        let mut driver_id = 0;

        let max_id = MAX_DRIVER_PORT - MIN_DRIVER_PORT;

        while driver_id <= max_id {
            let addr = get_driver_address_by_id(driver_id).map_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                e.to_string()
            })?;

            if let Ok(mut socket) = TcpStream::connect(addr.clone()).await {
                let request = serde_json::to_string(&CommonMessages::ResponseIdentification {
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

                Self::connect_with(self_id, central_driver_addr, socket, driver_id).await?;
            }

            driver_id += 1;
        }

        Ok(())
    }

    async fn setup(central_driver_addr: &Addr<CentralDriver>, id: u32) -> Result<(), String> {
        Self::connect_all_drivers(id, central_driver_addr).await?;

        // raise election
        central_driver_addr
            .try_send(StartElection {})
            .map_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                e.to_string()
            })?;

        let self_addr = get_driver_address_by_id(id).map_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            e.to_string()
        })?;

        log::info!("My addr is {}", self_addr);

        let listener = TcpListener::bind(self_addr).await.map_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            e.to_string()
        })?;

        log::info!("Listening to new connections!");

        loop {
            let (mut socket, addr) = listener.accept().await.map_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                e.to_string()
            })?;

            log::debug!("Connection accepted from {}", addr);

            let mut reader = BufReader::new(&mut socket);

            let mut str_response = String::new();

            reader.read_line(&mut str_response).await.map_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                e.to_string()
            })?;

            if str_response.is_empty() {
                return Err("Error receiving identification".into());
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
                CommonMessages::ResponseIdentification { id, type_ } => match type_ {
                    'D' => Self::connect_with(id, central_driver_addr, socket, id).await?,
                    _ => (),
                },
                _ => (),
            }
        }
    }

    async fn connect_with(
        self_id: u32,
        central_driver_addr: &Addr<CentralDriver>,
        socket: TcpStream,
        driver_id: u32,
    ) -> Result<(), String> {
        let driver_addr: Option<std::net::SocketAddr> = socket.peer_addr().ok();
        let (r, w) = split(socket);

        let driver_conn = DriverConnection::create(|ctx| {
            ctx.add_stream(LinesStream::new(BufReader::new(r).lines()));
            DriverConnection::new(
                self_id,
                central_driver_addr.clone(),
                Arc::new(Mutex::new(w)),
                driver_addr,
                driver_id,
            )
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
}

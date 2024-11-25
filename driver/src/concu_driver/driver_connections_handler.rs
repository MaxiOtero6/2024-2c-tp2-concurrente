use std::sync::{Arc, Mutex};

use actix::{Actor, Addr, AsyncContext};
use tokio::{
    io::{split, AsyncBufReadExt, BufReader},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_stream::wrappers::LinesStream;

use super::{
    central_driver::{CentralDriver, InsertDriverConnection},
    consts::{MAX_DRIVER_PORT, MIN_DRIVER_PORT},
    driver_connection::{DriverConnection, SendAll},
    json_parser::DriverMessages,
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
            let addr = get_driver_address_by_id(driver_id).map_err(|e| e.to_string())?;

            if let Ok(socket) = TcpStream::connect(addr.clone()).await {
                Self::connect_with(self_id, central_driver_addr, socket, Some(driver_id)).await?;
            }

            driver_id += 1;
        }

        Ok(())
    }

    async fn setup(central_driver_addr: &Addr<CentralDriver>, id: u32) -> Result<(), String> {
        Self::connect_all_drivers(id, central_driver_addr).await?;

        // raise election

        let self_addr = get_driver_address_by_id(id).map_err(|e| e.to_string())?;

        log::info!("My addr is {}", self_addr);

        let listener = TcpListener::bind(self_addr)
            .await
            .map_err(|e| e.to_string())?;

        log::info!("Listening to new connections!");

        loop {
            let (socket, addr) = listener.accept().await.map_err(|e| e.to_string())?;

            log::debug!("Connection accepted from {}", addr);

            Self::connect_with(id, central_driver_addr, socket, None).await?;
        }
    }

    async fn connect_with(
        self_id: u32,
        central_driver_addr: &Addr<CentralDriver>,
        socket: TcpStream,
        driver_id: Option<u32>,
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
            )
        });

        match driver_id {
            Some(did) => central_driver_addr
                .try_send(InsertDriverConnection {
                    id: did,
                    addr: driver_conn,
                })
                .map_err(|e| e.to_string()),

            None => {
                let parsed_json = serde_json::to_string(&DriverMessages::RequestDriverId {})
                    .map_err(|e| e.to_string())?;

                driver_conn
                    .send(SendAll { data: parsed_json })
                    .await
                    .map_err(|e| e.to_string())
            }
        }
    }
}

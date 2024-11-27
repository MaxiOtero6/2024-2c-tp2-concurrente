use std::{error::Error, thread::sleep, time::Duration};

use actix::System;
use rand::Rng;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};

use common::utils::{
    json_parser::{CommonMessages, TripMessages},
    position::Position,
};

use common::utils::consts::{HOST, MAX_DRIVER_PORT, MIN_DRIVER_PORT};

pub fn request_trip(id: u32) -> Result<(), Box<dyn Error>> {
    System::new().block_on(request(id))?;

    Ok(())
}

async fn request(id: u32) -> Result<(), Box<dyn Error>> {
    let mut ports: Vec<u32> = (MIN_DRIVER_PORT..=MAX_DRIVER_PORT).collect();
    let mut rng = rand::thread_rng();

    while !ports.is_empty() {
        let index = rng.gen_range(0..ports.len());
        let addr = format!("{}:{}", HOST, ports.remove(index));

        if let Ok(mut socket) = TcpStream::connect(addr.clone()).await {
            let identification =
                serde_json::to_string(&CommonMessages::Identification { id, type_: 'P' })?;

            socket.write_all((identification + "\n").as_bytes()).await?;

            let request = serde_json::to_string(&TripMessages::TripRequest {
                source: Position::new(50, 100),
                destination: Position::random(),
            })?;
            sleep(Duration::from_secs(1));
            socket.write_all((request + "\n").as_bytes()).await?;
            log::info!("REquest sent!");
            let mut reader = BufReader::new(&mut socket);

            let mut str_response = String::new();

            reader.read_line(&mut str_response).await.map_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                e.to_string()
            })?;

            if str_response.is_empty() {
                return Err("Error receiving trip response".into());
            }

            log::debug!("{}", str_response);

            break;
        }
    }

    Ok(())
}

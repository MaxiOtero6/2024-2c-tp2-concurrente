use std::error::Error;

use actix::System;
use tokio::{io::{AsyncBufReadExt, AsyncWriteExt, BufReader}, net::TcpListener};

use crate::concu_payment::{consts::{HOST, PORT}, json_parser::PaymentMessages};

pub fn handle_payments() -> Result<(), Box<dyn Error>> {
    System::new().block_on(handle())?;

    Ok(())
}

async fn handle() -> Result<(), Box<dyn Error>> {
    let auth_passengers = Vec::new();
    
    let self_addr = format!("{}:{}", HOST, PORT);

    log::info!("My addr is {}", self_addr);

    let listener = TcpListener::bind(self_addr).await.map_err(|e| {
        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        e.to_string()
    })?;

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

        let response: PaymentMessages = serde_json::from_str(&str_response).map_err(|e| {
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
            PaymentMessages::AuthPayment { id } => {
                
                //p aceptar
                auth_passengers.push(id);
                socket.write_all("eu te la acepte\n")
                // o 
                // p' de rebotar
                socket.write_all("no pinta hoy\n")
            },
            PaymentMessages::CollectPayment {id} => {
                if id in auth_passengers:
                    //log de que cobro
                    socket.write_all("eu le cobre")
                else:
                    socket.write_all("raterooo")
                    error

            }

        }
    }
}

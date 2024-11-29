use crate::concu_payment::{
    consts::{HOST, PORT}, };
use common::utils::json_parser::{PaymentMessages, PaymentResponses};
use rand::Rng;
use std::error::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
pub(crate) async fn handle_payments() -> Result<(), Box<dyn Error>> {
    handle().await
}

async fn handle() -> Result<(), Box<dyn Error>> {
    let mut auth_passengers = Vec::new();

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

        let _ = reader.read_line(&mut str_response).await.inspect_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        });

        if str_response.is_empty() {
            log::error!("Error reading response");
            continue;
        }

        let response: PaymentMessages = match serde_json::from_str(&str_response) {
            Ok(msg) => msg,
            Err(e) => {
                log::error!("Failed to parse PaymentMessages: {}, str: {}", e, str_response);
                continue;
            }
        };


        match response {
            PaymentMessages::AuthPayment { passenger_id } => {
                handle_auth_message(&mut auth_passengers, &mut socket, passenger_id).await?;
            }
            PaymentMessages::CollectPayment {
                driver_id,
                passenger_id,
            } => {
                handle_collect_message(&mut auth_passengers, &mut socket, driver_id, &passenger_id)
                    .await?;
            }
        }
    }
}

async fn handle_collect_message(auth_passengers: &mut Vec<u32>, socket: &mut TcpStream, driver_id: u32, passenger_id: &u32) -> Result<(), Box<dyn Error>> {
        let response_message = if auth_passengers.contains(passenger_id) {
            log::debug!(
                        "Driver {} collected payment from passenger {}",
                        driver_id,
                        passenger_id
                    );

            PaymentResponses::CollectPayment {
                passenger_id: *passenger_id,
                response: true,
            }
        } else {
            log::debug!(
                        "Driver {} could not collect payment from passenger {}",
                        driver_id,
                        passenger_id
                    );

            PaymentResponses::CollectPayment {
                passenger_id: *passenger_id,
                response: false,
            }
        };

        let response_json = serialize_response_message(&response_message)?;
        send_response(socket, response_json).await;
        Ok(())
    }

    async fn handle_auth_message(auth_passengers: &mut Vec<u32>, mut socket: &mut TcpStream, passenger_id: u32) -> Result<(), Box<dyn Error>> {
        let mut rng = rand::thread_rng();
        let probability: bool = rng.gen_bool(0.7);

        if probability {
            auth_passengers.push(passenger_id);
            log::debug!("Accepted payment from passenger {}", passenger_id);

            let response_message = PaymentResponses::AuthPayment {
                passenger_id,
                response: true,
            };

            let response_json = serialize_response_message(&response_message)?;
            send_response(socket, response_json).await;
        } else {
            log::debug!("Rejected payment from passenger {}", passenger_id);

            let response_message = PaymentResponses::AuthPayment {
                passenger_id,
                response: false,
            };
            let response_json = serialize_response_message(&response_message)?;
            send_response(socket, response_json).await;
        }
        Ok(())
    }

    async fn send_response(socket: &mut TcpStream, response_json: String) {
        if let Err(e) = socket.write_all((response_json + "\n").as_bytes()).await {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        }
    }

    fn serialize_response_message(
        response_message: &PaymentResponses,
    ) -> Result<String, Box<dyn Error>> {
        serde_json::to_string(&response_message).map_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            e.into()
        })
    }


/*
#[tokio::test]
async fn test_payment_functionality() -> Result<(), Box<dyn Error>> {
    // Start the server in a separate task
    tokio::spawn(async {
        handle_payments().await?;
    });

    // Give the server some time to start
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Connect to the server
    let mut stream = TcpStream::connect("127.0.0.1:3000").await?;

    // Create a request message
    let request_message = PaymentMessages::AuthPayment { passenger_id: 1 };
    let request_json = serde_json::to_string(&request_message)?;

    // Send the request
    stream.write_all((request_json + "\n").as_bytes()).await?;

    // Read the response
    let mut reader = BufReader::new(&mut stream);
    let mut response_json = String::new();
    reader.read_line(&mut response_json).await?;

    // Deserialize the response
    let response: PaymentResponses = serde_json::from_str(&response_json)?;

    // Verify the response
    match response {
        PaymentResponses::AuthPayment { passenger_id, response } => {
            assert_eq!(passenger_id, 1);
            assert!(response);
        }
        _ => panic!("Unexpected response"),
    }

    Ok(())
}
*/
 
use rand::Rng;
use std::error::Error;
use std::sync::{Arc};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::spawn;
use tokio::sync::RwLock;
use crate::concu_payment::json_parser::PaymentResponses;
use crate::concu_payment::{
    consts::{HOST, PORT},
    json_parser::PaymentMessages,
};

#[tokio::main]
pub(crate) async fn handle_payments() -> Result<(),Box<dyn std::error::Error + Send + Sync>> {
    handle().await
}

async fn handle() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let auth_passengers = Arc::new(RwLock::new(Vec::new()));

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

        let auth_passengers_clone = Arc::clone(&auth_passengers);

        spawn(async move {
            
            let mut reader = BufReader::new(&mut socket);

            let mut str_response = String::new();

            reader.read_line(&mut str_response).await.map_err(|e| {
                log::error!("Error reading line: {}", e);
                e.to_string()
            })?;
        /*
            if str_response.is_empty() {
                return Err("Error receiving identification".into());
            }
        */
            
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
                
                PaymentMessages::AuthPayment { passenger_id } => {
                    let mut rng = rand::thread_rng();
                    let probability: bool = rng.gen_bool(0.7);

                    if probability {
                        if let mut auth_list = auth_passengers_clone.write().await {
                            log::debug!("Taking the lock");
                            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                            auth_list.push(passenger_id);
                        }
                        log::debug!("free the lock");

                        log::debug!("Accepted payment from passenger {}", passenger_id);

                        let response_message: PaymentResponses =
                            PaymentResponses::AuthPayment { passenger_id, response: true };
                        let response_json = serialize_response_message(&response_message);
                        
                        match response_json {
                            Ok(response_message) => {
                                if let Err(e) = socket.write_all((response_message + "\n").as_bytes()).await {
                                    log::error!("Error writing to {}: {}", addr, e.to_string());
                                    return Err(format!("Error writing to {}, reason: {}", addr, e.to_string()).into());
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to serialize response message: {}", e);
                                return Err(format!("Failed to serialize response message: {}", e).into());
                            }
                        };
                        
                        
                    } else {
                        log::debug!("Rejected payment from passenger {}", passenger_id);
                        let response_message: PaymentResponses = PaymentResponses::AuthPayment {
                            passenger_id,
                            response: false,
                        };

                        let response_json = serialize_response_message(&response_message);

                        match response_json {
                            Ok(response_message) => {
                                if let Err(e) = socket.write_all((response_message + "\n").as_bytes()).await {
                                    log::error!("Error writing to {}: {}", addr, e.to_string());
                                    return Err(format!("Error writing to {}, reason: {}", addr, e.to_string()).into());
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to serialize response message: {}", e);
                                return Err(format!("Failed to serialize response message: {}", e).into());
                            }
                        };
                    }
                    Ok::<(), Box<dyn Error>>(())
                }

                PaymentMessages::CollectPayment { driver_id, passenger_id } => {
                    let auth_list = auth_passengers_clone.read().await;

                    let response_message = if auth_list.contains(&passenger_id) {
                        log::debug!("Driver {} collected payment from passenger {}", driver_id, passenger_id);
                        let response_message: PaymentResponses =
                            PaymentResponses::CollectPayment { passenger_id, response: true };

                        serialize_response_message(&response_message)
                    } else {
                        log::debug!("Driver {} could not collect payment from passenger {}", driver_id, passenger_id);

                        let response_message: PaymentResponses = PaymentResponses::CollectPayment {
                            passenger_id,
                            response: false,
                        };

                        serialize_response_message(&response_message)
                    };

                    match response_message {
                        Ok(response_message) => {
                            if let Err(e) = socket.write_all((response_message + "\n").as_bytes()).await {
                                log::error!("Error writing to {}: {}", addr, e.to_string());
                                return Err(format!("Error writing to {}, reason: {}", addr, e.to_string()).into());
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to serialize response message: {}", e);
                            return Err(format!("Failed to serialize response message: {}", e).into());
                        }
                    };

                    Ok::<(), Box<dyn Error>>(())
                }
            }.expect("TODO: panic message");
            Ok(())
        });
       
    }
    
}

fn serialize_response_message(response_message: &PaymentResponses) -> Result<String, Box<dyn Error>> {
    serde_json::to_string(&response_message).map_err(|e| {
        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        e.into()
    })
}


#[tokio::test]

async fn test_server() -> Result<(), Box<dyn Error>> {
    // Start the server in a separate task
    tokio::spawn(async {
        handle_payments().unwrap();
    });

    // Give the server some time to start
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Connect to the server
    let mut stream = TcpStream::connect("127.0.0.1:8080").await?;

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




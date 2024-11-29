use rand::Rng;
use std::{error::Error, thread::sleep, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};

use common::utils::json_parser::{CommonMessages, TripMessages};

use crate::concu_passenger::utils::TripData;
use common::utils::consts::{
    HOST, MAX_DRIVER_PORT, MIN_DRIVER_PORT, MIN_PASSENGER_PORT, PAYMENT_PORT,
};
use common::utils::json_parser::{PaymentMessages, PaymentResponses};
use tokio::net::TcpListener;
use tokio::time::timeout;

async fn validate_credit_card(id: u32) -> Result<(), Box<dyn Error>> {
    validate(id).await?;
    Ok(())
}
async fn request_trip(trip_data: TripData) -> Result<(), Box<dyn Error>> {
    request(trip_data).await?;
    Ok(())
}

#[tokio::main]
pub(crate) async fn handle_complete_trip(trip_data: TripData) -> Result<(), Box<dyn Error>> {
    validate_credit_card(trip_data.id).await?;
    request_trip(trip_data).await?;
    Ok(())
}

async fn validate(id: u32) -> Result<(), Box<dyn Error>> {
    let payment_port: u32 = PAYMENT_PORT;

    let addr = format!("{}:{}", HOST, payment_port);

    if let Ok(mut socket) = TcpStream::connect(addr.clone()).await {
        log::info!("Connected to payment server");
        send_auth_message(&id, &mut socket).await?;
        handle_payment_response(&mut socket).await?;
    } else {
        log::error!("Error connecting to payment server");
    }

    Ok(())
}
async fn handle_payment_response(socket: &mut TcpStream) -> Result<(), Box<dyn Error>> {
    let mut reader = BufReader::new(socket);
    let str_response = wait_driver_response(
        &mut reader,
        "Error receiving validation response".parse().unwrap(),
    )
    .await?;

    let response: PaymentResponses = match serde_json::from_str(&str_response) {
        Ok(msg) => msg,
        Err(e) => {
            log::error!(
                "Failed to parse PaymentMessages: {}, str: {}",
                e,
                str_response
            );
            return Err("Error parsing response".into());
        }
    };

    match response {
        PaymentResponses::AuthPayment { response, .. } => {
            if response {
                log::info!("Credit card validated!");
            } else {
                log::error!("Invalid Credit Card!");
                return Err("Payment was rejected. Exiting the program.".into());
            }
        }
        _ => {
            log::error!("Invalid response");
            return Err("Invalid response".into());
        }
    }
    Ok(())
}
async fn send_auth_message(id: &u32, socket: &mut TcpStream) -> Result<(), Box<dyn Error>> {
    let payment_auth_message =
        serde_json::to_string(&PaymentMessages::AuthPayment { passenger_id: *id })?;

    socket
        .write_all((payment_auth_message + "\n").as_bytes())
        .await?;
    Ok(())
}

async fn request(trip_data: TripData) -> Result<(), Box<dyn Error>> {
    let mut ports: Vec<u32> = (MIN_DRIVER_PORT..=MAX_DRIVER_PORT).collect();
    let mut rng = rand::thread_rng();
    log::info!("Requesting trip");

    let self_addr = { format!("{}:{}", HOST, MIN_PASSENGER_PORT + trip_data.id) };

    while !ports.is_empty() {
        let index = rng.gen_range(0..ports.len());
        let addr = format!("{}:{}", HOST, ports.remove(index));

        if let Ok(mut socket) = TcpStream::connect(addr.clone()).await {
            send_identification(&trip_data, &mut socket).await?;
            log::info!("Identification sent!");

            send_trip_request(&mut socket, &trip_data).await?;
            log::info!("Request sent!");
        } else {
            continue;
        }

        let listener = TcpListener::bind(&self_addr).await.map_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            e.to_string()
        })?;

        loop {
            let result = timeout(Duration::from_secs(3), listener.accept()).await;

            match result {
                Ok(Ok((mut socket, _))) => {
                    log::info!("Connection accepted");
                    wait_driver_responses(&mut socket).await?;
                }
                Ok(Err(e)) => {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                }
                Err(_) => {
                    log::warn!("No response from driver");
                    log::info!("Trying to connect again");
                    break;
                }
            }
        }
    }
    Ok(())
}

async fn wait_driver_responses(socket: &mut TcpStream) -> Result<(), Box<dyn Error>> {
    let mut reader = BufReader::new(socket);
    loop {
        let string_response = wait_driver_response(
            &mut reader,
            "Error receiving trip response".parse().unwrap(),
        )
        .await;

        let response = match parse_trip_response(string_response?) {
            Ok(value) => value,
            Err(value) => return value,
        };

        match response {
            TripMessages::TripResponse { status, detail } => match status {
                common::utils::json_parser::TripStatus::Success => {
                    log::info!("Trip success: {}", detail);
                    continue;
                }
                common::utils::json_parser::TripStatus::DriverSelected => {
                    log::info!("Driver selected: {}", detail);
                    break;
                }
                common::utils::json_parser::TripStatus::Error => {
                    log::error!("Trip error: {}", detail);
                    break;
                }
                common::utils::json_parser::TripStatus::RequestDelivered => {
                    break;
                }
            },
            _ => {
                log::error!("Invalid response");
                break;
            }
        }
    }
    Ok(())
}
fn parse_trip_response(response: String) -> Result<TripMessages, Result<(), Box<dyn Error>>> {
    let response: TripMessages = match serde_json::from_str(&response) {
        Ok(msg) => msg,
        Err(e) => {
            log::error!("Failed to parse PaymentMessages: {}, str: {}", e, response);
            return Err(Err("Error parsing response".into()));
        }
    };
    Ok(response)
}
async fn wait_driver_response(
    reader: &mut BufReader<&mut TcpStream>,
    error: String,
) -> Result<String, Box<dyn Error>> {
    let mut str_response = String::new();

    reader.read_line(&mut str_response).await.map_err(|e| {
        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        e.to_string()
    })?;

    if str_response.is_empty() {
        return Err(error.into());
    }
    Ok(str_response)
}
async fn send_trip_request(
    socket: &mut TcpStream,
    request: &TripData,
) -> Result<(), Box<dyn Error>> {
    let request = serde_json::to_string(request)?;

    sleep(Duration::from_secs(1));
    socket.write_all((request + "\n").as_bytes()).await?;
    Ok(())
}

async fn send_identification(
    trip_data: &TripData,
    socket: &mut TcpStream,
) -> Result<(), Box<dyn Error>> {
    let identification = serde_json::to_string(&CommonMessages::Identification {
        id: trip_data.id,
        type_: 'P',
    })?;

    socket.write_all((identification + "\n").as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::utils::json_parser::{CommonMessages, TripMessages};
    use common::utils::position::Position;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
    };

    const MOCK_SERVER_HOST: &str = "127.0.0.1";
    const MOCK_SERVER_PORT: u32 = 8080;

    // Mock para simular el servidor
    async fn mock_server() {
        let addr = format!("{}:{}", MOCK_SERVER_HOST, MOCK_SERVER_PORT);
        let listener = TcpListener::bind(&addr).await.unwrap();

        if let Ok((mut socket, _)) = listener.accept().await {
            let mut reader = BufReader::new(&mut socket);
            let mut buffer = String::new();

            // Leer mensaje de identificación
            reader.read_line(&mut buffer).await.unwrap();
            let identification: CommonMessages = serde_json::from_str(&buffer.trim()).unwrap();

            // Validar mensaje y continuar
            match identification {
                CommonMessages::Identification { id, type_ } => {
                    assert_eq!(id, 1);
                    assert_eq!(type_, 'P');
                }
            }

            // Leer solicitud de viaje
            buffer.clear();
            reader.read_line(&mut buffer).await.unwrap();
            let trip_request: TripMessages = serde_json::from_str(&buffer.trim()).unwrap();

            if let TripMessages::TripRequest {
                source,
                destination,
            } = trip_request
            {
                assert_eq!(source.x, 50);
                assert_eq!(source.y, 100);
                assert_eq!(destination.x, 150);
                assert_eq!(destination.y, 200);
            } else {
                panic!("Mensaje de solicitud de viaje inválido");
            }

            // Enviar respuesta simulada
            let response = "{\"status\":\"accepted\"}\n";
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_request_trip_with_mock() {
        // Inicia el servidor mock en una tarea separada
        let server_task = tokio::spawn(mock_server());

        // Simula la conexión del cliente
        let result = request_trip(TripData {
            id: 1,
            origin: Position { x: 50, y: 100 },
            destination: Position { x: 150, y: 200 },
        })
        .await;
        assert!(result.is_ok());

        // Espera a que el servidor mock termine
        server_task.await.unwrap();
    }
}

use rand::Rng;
use std::{error::Error, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    join,
    net::TcpStream,
    task,
};

use common::utils::json_parser::{CommonMessages, TripMessages};

use crate::concu_passenger::utils::TripData;
use common::utils::consts::{
    HOST, MAX_DRIVER_PORT, MIN_DRIVER_PORT, MIN_PASSENGER_PORT, PAYMENT_PORT,
};
use common::utils::json_parser::{PaymentMessages, PaymentResponses};
use tokio::net::TcpListener;
use tokio::time::timeout;


/// Valida la tarjeta de crédito del pasajero
async fn validate_credit_card(id: u32) -> Result<(), Box<dyn Error>> {
    validate(id).await?;
    Ok(())
}

/// Realiza una solicitud de viaje al servidor de conductores
async fn request_trip(trip_data: TripData) -> Result<(), Box<dyn Error>> {
    request(trip_data).await?;
    Ok(())
}

/// Maneja el proceso de completar un viaje
#[tokio::main]
pub(crate) async fn handle_complete_trip(trip_data: TripData) -> Result<(), Box<dyn Error>> {
    validate_credit_card(trip_data.id).await?;
    request_trip(trip_data).await?;
    Ok(())
}

/// Valida la tarjeta de crédito del pasajero.
/// Se conecta al servidor de pagos, envía un mensaje de autenticación y
/// espera la respuesta del servidor
/// - Si la respuesta es afirmativa, la tarjeta fue validada
/// - Si la respuesta es negativa, la tarjeta fue rechazada
/// - En caso de error, se retorna un error
async fn validate(id: u32) -> Result<(), Box<dyn Error>> {
    let payment_port: u32 = PAYMENT_PORT;

    let addr = format!("{}:{}", HOST, payment_port);

    if let Ok(mut socket) = TcpStream::connect(addr.clone()).await {
        log::info!("Connected to payment server");
        send_auth_message(&id, &mut socket).await?;
        handle_payment_response(&mut socket).await?;
    } else {
        log::error!("Error connecting to payment server");
        return Err("Error connecting to payment server: Exiting the program.".into());
    }

    Ok(())
}

/// Maneja la respuesta del servidor de pagos.
/// - Si la respuesta es afirmativa, la tarjeta fue validada
/// - Si la respuesta es negativa, la tarjeta fue rechazada
/// - En caso de error, se retorna un error
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

/// Envia un mensaje de autenticación al servidor de pagos
async fn send_auth_message(id: &u32, socket: &mut TcpStream) -> Result<(), Box<dyn Error>> {
    let payment_auth_message =
        serde_json::to_string(&PaymentMessages::AuthPayment { passenger_id: *id })?;

    socket
        .write_all((payment_auth_message + "\n").as_bytes())
        .await?;
    Ok(())
}

/// Crea un listener en un puerto específico y espera la conexión de un conductor por un período de tiempo.
/// Si la conexión es exitosa, se espera la respuesta del conductor
/// - Si la respuesta es afirmativa, el viaje fue completado y sale del loop
/// - Si la respuesta es negativa, el viaje fue rechazado
/// - Si no hay respuesta, se retorna un error
///
/// Si la conexión falla, se retorna un error.

async fn listen_connections(id: u32) -> Result<Result<(), String>, String> {
    let self_addr =  format!("{}:{}", HOST, MIN_PASSENGER_PORT + id) ;
    let listener = TcpListener::bind(&self_addr).await.map_err(|e| {
        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        e.to_string()
    })?;

    log::info!("My addr is {}", self_addr);

    loop {
        let result = timeout(Duration::from_secs(10), listener.accept()).await;

        match result {
            Ok(Ok((mut socket, _))) => {
                log::info!("Connection accepted");
                match wait_driver_responses(&mut socket).await {
                    Ok(Ok(_)) => {
                        log::info!("We arrived at your destination!");
                        break;
                    }
                    Ok(Err(e)) => return Ok(Err(e)),
                    Err(_broken_pipe) => {
                        return Err("Driver disconnected!, requesting trip again".into())
                    }
                }
            }
            Ok(Err(e)) => {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            }
            Err(_) => {
                log::warn!("No response from a driver");
                return Err("No response from a driver!, requesting trip again".into());
            }
        }
    }

    Ok(Ok(()))
}

/// Realiza una solicitud de viaje al servidor de conductores
/// - Envia un mensaje de identificación
/// - Envia un mensaje de solicitud de viaje
/// - Espera las respuestas de los conductores
async fn make_request(trip_data: &TripData, socket: &mut TcpStream) -> Result<(), Box<dyn Error>> {
    send_identification(&trip_data, socket).await?;

    log::info!("Identification sent!");

    send_trip_request(socket, &trip_data).await?;

    log::info!("Request sent!");

    wait_driver_responses(socket).await??;

    Ok(())
}



/// Itera por cada uno de los puertos de los conductores, intentando conectarse a cada uno de ellos
/// hasta que se logre una conexión exitosa o hasta que se agoten los puertos.
/// - Si la conexión es exitosa, envía un mensaje de solicitud de viaje
/// - Si la conexión falla, intenta conectarse a otro conductor
/// - Si no hay conductores disponibles, retorna un error
///
/// Una vez que se logra una conexión exitosa, se espera que un conductor se contacte con el pasajero  abriendo una nueva conexión TCP
async fn request(trip_data: TripData) -> Result<(), Box<dyn Error>> {
    let mut ports: Vec<u32> = (MIN_DRIVER_PORT..=MAX_DRIVER_PORT).collect();
    let mut rng = rand::thread_rng();
    log::info!("Requesting trip");

    let mut ret: Result<(), Box<dyn Error>> = Err("Oops!, I can't request a trip correctly".into());

    while !ports.is_empty() {
        let index = rng.gen_range(0..ports.len());
        let addr = format!("{}:{}", HOST, ports.remove(index));

        let mut socket = match TcpStream::connect(addr.clone()).await {
            Err(_) => continue,
            Ok(socket) => socket,
        };

        let listen_task = task::spawn(listen_connections(trip_data.id.clone()));

        if let Err(e) = make_request(&trip_data, &mut socket).await {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            listen_task.abort();
            continue;
        }

        let (listen_task_result,) = join!(listen_task);

        match listen_task_result {
            Ok(Err(e)) => log::error!("{}", e.to_string()), // Broken pipe
            Ok(Ok(Ok(_))) => { // Ok
                ret = Ok(());
                break;
            }
            Ok(Ok(Err(e))) => { // Negative response from driver
                log::error!("{}", e.to_string());
                break;
            }
            Err(e) => log::error!("{}", e.to_string()),
        }
    }

    ret
}

// TODO
/// Espera las respuestas de los conductores dentro de un loop
/// - Si la respuesta es afirmativa puede ser:
///    - Que el viaje fue aceptado  o completado  se sale del loop
///    - Que el viaje fue rechazado, se retorna un error
///    -
/// - Si la respuesta es negativa, el viaje fue rechazado y retorna un error
/// - Si no hay respuesta, se retorna un error
/// - Si la conexión falla, se retorna un error
///
async fn wait_driver_responses(socket: &mut TcpStream) -> Result<Result<(), String>, String> {
    let mut reader = BufReader::new(socket);

    let mut request_delivered = false;

    loop {
        let string_response =
            wait_driver_response(&mut reader, "Error receiving trip response".into()).await;

        if let Err(res) = string_response {
            if request_delivered {
                return Ok(Ok(()));
            } else {
                return Err(res.to_string());
            }
        }

        let response = match parse_trip_response(string_response.map_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            e.to_string()
        })?) {
            Ok(value) => value,
            Err(value) => return Err(value),
        };

        match response {
            TripMessages::TripResponse { status, detail } => match status {
                common::utils::json_parser::TripStatus::Success => {
                    log::info!("{}", detail);
                    return Ok(Ok(()));
                }
                common::utils::json_parser::TripStatus::Info => {
                    log::info!("{}", detail);
                }
                common::utils::json_parser::TripStatus::Error => {
                    return Ok(Err(detail));
                }
                common::utils::json_parser::TripStatus::RequestDelivered => {
                    log::info!("{}", detail);
                    request_delivered = true;
                }
            },
            _ => {
                log::error!("Invalid response");
                break;
            }
        }
    }
    Ok(Ok(()))
}

/// Parsea la respuesta del servidor de conductores
/// - Si la respuesta es un mensaje de error, retorna un error
/// - Si la respuesta es un mensaje de éxito, retorna la respuesta
fn parse_trip_response(response: String) -> Result<TripMessages, String> {
    let response: TripMessages = match serde_json::from_str(&response) {
        Ok(msg) => msg,
        Err(e) => {
            log::error!("Failed to parse PaymentMessages: {}, str: {}", e, response);
            return Err("Error parsing response".into());
        }
    };
    Ok(response)
}

/// Espera la respuesta del servidor de conductores y la retorna
/// - Si la respuesta es vacía, retorna un error
/// - Si la respuesta es un mensaje de error, retorna un error
/// - Si la respuesta es un mensaje de éxito, retorna la respuesta
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

/// Convierte la request de petición de viaje en un string y lo envia a través del socket
async fn send_trip_request(
    socket: &mut TcpStream,
    request: &TripData,
) -> Result<(), Box<dyn Error>> {
    let request = serde_json::to_string(&TripMessages::TripRequest {
        source: request.origin,
        destination: request.destination,
    })?;

    socket
        .write_all((request + "\n").as_bytes())
        .await
        .inspect_err(|e| log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string()))?;
    Ok(())
}

/// Envia un mensaje de identificación, a través del socket, al servidor de conductores
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

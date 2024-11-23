use std::error::Error;

use crate::concu_driver::consts::{HOST, MAX_DRIVER_PORT, MIN_DRIVER_PORT};

pub fn get_driver_address_by_id(id: u32) -> Result<String, Box<dyn Error>> {
    fn get_port_by_id(id: u32) -> u32 {
        MIN_DRIVER_PORT + id
    }

    get_driver_address(get_port_by_id(id))
}

pub fn get_driver_address(port: u32) -> Result<String, Box<dyn Error>> {
    if port < MIN_DRIVER_PORT || port > MAX_DRIVER_PORT {
        return Err(format!(
            "Wrong port number, valid port numbers are between {} <= port <= {}",
            MIN_DRIVER_PORT, MAX_DRIVER_PORT
        )
        .into());
    }

    Ok(format!("{}:{}", HOST, port))
}

pub fn get_id_by_port(port: String) -> Result<u32, Box<dyn Error>> {
    let parsed_port = port.parse::<u32>()?;

    if parsed_port < MIN_DRIVER_PORT || parsed_port > MAX_DRIVER_PORT {
        return Err(format!(
            "Wrong port number, valid port numbers are between {} <= port <= {}",
            MIN_DRIVER_PORT, MAX_DRIVER_PORT
        )
        .into());
    }

    Ok(parsed_port - MIN_DRIVER_PORT)
}

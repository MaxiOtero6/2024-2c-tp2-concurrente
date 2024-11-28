use std::error::Error;
use concu_passenger::input_handler;
use concu_passenger::passenger::request_trip;
use crate::concu_passenger::passenger::validate_credit_card;

pub mod concu_passenger;

pub fn run() -> Result<(), Box<dyn Error>> {
    
    match input_handler::validate_args() {
        Ok(trip_data) => {
            log::info!("Validated trip data: {:?}", trip_data);
            
            let _ = validate_credit_card(trip_data.id.clone());
            request_trip(trip_data)
            
        }
        Err(error) => {
            eprintln!("{}", error);
            Err(Box::from(error))
        }
    }
    
}


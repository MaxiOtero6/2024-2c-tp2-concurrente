use concu_passenger::input_handler;
use concu_passenger::passenger::handle_complete_trip;
use std::error::Error;

pub mod concu_passenger;

pub fn run() -> Result<(), Box<dyn Error>> {
    match input_handler::validate_args() {
        Ok(trip_data) => {
            log::info!("Validated trip data: {:?}", trip_data);
            handle_complete_trip(trip_data)?;
            Ok(())
        }
        Err(error) => {
            eprintln!("{}", error);
            Err(Box::from(error))
        }
    }
}

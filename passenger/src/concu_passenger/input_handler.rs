use std::env;
use regex::Regex;
use common::utils::position::Position;
use crate::concu_passenger::utils::TripData;

pub fn validate_args() -> Result<TripData, String> {

    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.join(" ");

    let command_pattern = Regex::new(
        r"^id=(\d+)\s+origin=\((-?\d+),(-?\d+)\)\s+dest=\((-?\d+),(-?\d+)\)$"
    ).expect("Regex no válida");

    if let Some(captures) = command_pattern.captures(&command) {
        let id: u32 = captures[1].parse().expect("Invalid ID number");
        let origin_x: u32 = captures[2].parse().expect("Invalid origin X");
        let origin_y: u32 = captures[3].parse().expect("Invalid origin Y ");
        let destination_x:u32 = captures[4].parse().expect("Invalid destination X ");
        let destination_y:u32 = captures[5].parse().expect("Invalid destination Y ");

        Ok(TripData {
            id,
            origin: Position::new(origin_x, origin_y),
            destination: Position::new(destination_x, destination_y)
        })
    } else {
        Err("Invalid command format.".to_string())
    }
}

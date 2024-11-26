use std::error::Error;

use concu_passenger::passenger::request_trip;

pub mod concu_passenger;

pub fn run() -> Result<(), Box<dyn Error>> {
    let argv: Vec<String> = std::env::args().collect();

    // if argv.len() != 2 {
    //     return Err("Wrong args, expected: <program> <driver_id>".into());
    // }

    // let driver_id = argv[1]
    //     .parse::<u32>()
    //     .expect("Wrong driver_id, value must be parseable to u32");

    request_trip()
}

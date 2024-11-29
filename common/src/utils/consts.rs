use log::LevelFilter;

pub const HOST: &str = "0.0.0.0";

pub const MIN_DRIVER_PORT: u32 = 8080;
pub const MAX_DRIVER_PORT: u32 = 8100;
pub const MIN_PASSENGER_PORT: u32 = 8000;
pub const MAX_PASSENGER_PORT: u32 = 8020;
pub const PAYMENT_PORT: u32 = 3000;
pub const LOG_LEVEL: LevelFilter = LevelFilter::Debug;



use std::time::Duration;

use log::LevelFilter;

pub const LEADER_PORT: u32 = 8080;
pub const HOST: &str = "0.0.0.0";
pub const MIN_DRIVER_PORT: u32 = 8081;
pub const MAX_DRIVER_PORT: u32 = 8101;
pub const MAX_DISTANCE: u32 = 10;
pub const PAYMENT_PORT: u32 = 3000;
pub const LOG_LEVEL: LevelFilter = LevelFilter::Debug;
pub const ELECTION_TIMEOUT_DURATION: Duration = Duration::from_secs(1);

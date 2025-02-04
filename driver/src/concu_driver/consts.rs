use std::time::Duration;

pub const MAX_DISTANCE: u32 = 10;
pub const ELECTION_TIMEOUT_DURATION: Duration = Duration::from_secs(1);
pub const POSITION_NOTIFICATION_INTERVAL: Duration = Duration::from_secs(5);
pub const TAKE_TRIP_TIMEOUT_MS: Duration = Duration::from_millis(300);
pub const DEFAULT_TAKE_TRIP_PROBABILTY: f64 = 1.0;
pub const TRIP_GO_TO_SLEEP: Duration = Duration::from_millis(750);

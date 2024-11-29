use serde::{Deserialize, Serialize};

use super::position::Position;

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum TripStatus {
    RequestDelivered,
    DriverSelected,
    Success,
    Error,
}

#[derive(Serialize, Deserialize)]
pub enum CommonMessages {
    Identification { id: u32, type_: char },
}

#[derive(Serialize, Deserialize)]
pub enum TripMessages {
    TripRequest {
        source: Position,
        destination: Position,
    },
    TripResponse {
        status: TripStatus,
        detail: String,
    },
}

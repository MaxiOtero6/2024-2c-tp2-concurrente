use serde::{Deserialize, Serialize};

use super::position::Position;

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum TripStatus {
    RequestDelivered,
    Info,
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

#[derive(Deserialize, Serialize)]
pub enum PaymentMessages {
    AuthPayment { passenger_id: u32 },
    CollectPayment { driver_id: u32, passenger_id: u32 },
}

#[derive(Serialize, Deserialize, Debug)]
pub enum PaymentResponses {
    AuthPayment { passenger_id: u32, response: bool },
    CollectPayment { passenger_id: u32, response: bool },
}

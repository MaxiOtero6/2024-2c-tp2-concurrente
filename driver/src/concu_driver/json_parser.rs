use serde::{Deserialize, Serialize};

use common::utils::{json_parser::TripStatus, position::Position};

#[derive(Serialize, Deserialize)]
pub enum CommonMessages {
    Identification { id: u32, type_: char },
}

#[derive(Serialize, Deserialize)]
pub enum DriverMessages {
    Coordinator {
        leader_id: u32,
    },
    Election {
        sender_id: u32,
    },
    Alive {
        responder_id: u32,
    },
    NotifyPosition {
        driver_id: u32,
        driver_position: Position,
    },
    TripRequest {
        passenger_id: u32,
        passenger_location: Position,
        destination: Position,
        first_contact_driver: u32,
    },
    CanHandleTrip {
        passenger_id: u32,
        passenger_location: Position,
        destination: Position,
        first_contact_driver: u32,
    },
    CanHandleTripACK {
        response: bool,
        passenger_id: u32,
    },
    TripStatus {
        passenger_id: u32,
        status: TripStatus,
        detail: String,
    },
}

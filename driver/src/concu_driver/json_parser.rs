use serde::{Deserialize, Serialize};

use common::utils::position::Position;

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
    },
    CanHandleTrip {
        passenger_id: u32,
        driver_id: u32,
        passenger_location: Position,
        destination: Position,
    },
    CanHandleTripACK {
        response: bool,
        passenger_id: u32,
        driver_id: u32,
    },
}

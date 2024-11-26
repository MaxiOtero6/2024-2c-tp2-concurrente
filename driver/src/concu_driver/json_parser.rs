use serde::{Deserialize, Serialize};

use super::position::Position;

#[derive(Serialize, Deserialize)]
pub enum CommonMessages {
    RequestIdentification {},
    ResponseIdentification { id: u32, type_: char },
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
}

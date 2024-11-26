use serde::{Deserialize, Serialize};

use super::position::Position;

#[derive(Serialize, Deserialize)]
pub enum DriverMessages {
    RequestDriverId {},
    ResponseDriverId { id: u32 },
    Coordinator { leader_id: u32 },
    Election { sender_id: u32 },
    Alive { responder_id: u32 },
    NotifyPosition {driver_id: u32, driver_position: Position}
}

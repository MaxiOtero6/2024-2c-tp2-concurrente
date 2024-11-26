use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum DriverMessages {
    RequestDriverId {},
    ResponseDriverId { id: u32 },
    Coordinator { leader_id: u32 },
    Election { sender_id: u32 },
    Alive { responder_id: u32 },
}

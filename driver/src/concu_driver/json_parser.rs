use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum DriverMessages {
    RequestDriverId {},
    ResponseDriverId { id: u32 },
}


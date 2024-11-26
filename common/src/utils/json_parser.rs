use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum CommonMessages {
    Identification { id: u32, type_: char },
}

use common::utils::position::Position;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TripData {
    pub id: u32,
    pub origin: Position,
    pub destination: Position,
}

use common::utils::position::Position;

#[derive(Debug)]
pub struct TripData {
    pub id: u32,
    pub origin: Position,
    pub destination: Position
}

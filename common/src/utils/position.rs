use std::u32;

use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: u32,
    pub y: u32,
}

impl Position {
    /// Crea una nueva posición
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    /// Crea una nueva posición con valores aleatorios
    pub fn random() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            x: rng.gen_range(0..100),
            y: rng.gen_range(0..100),
        }
    }

    /// Crea una nueva posición con valores infinitos
    pub fn infinity() -> Self {
        Self {
            x: u32::MAX,
            y: u32::MAX,
        }
    }

    /// Calcula la distancia entre dos posiciones
    pub fn distance_to(&self, p: &Position) -> u32 {
        if self.x > 100 || p.x > 100 || self.y > 100 || p.y > 100 {
            return u32::MAX;
        }

        let x: u32 = (p.x as i32 - self.x as i32).abs() as u32;
        let y: u32 = (p.y as i32 - self.y as i32).abs() as u32;

        x + y
    }

    /// Mueve la posición hacia otra posición
    pub fn go_to(&mut self, p: &Position) {
        let mut rng = rand::thread_rng();
        let step_x: u32 = rng.gen_range(0..=3);
        let step_y: u32 = rng.gen_range(0..=3);

        let dx = p.x as i32 - self.x as i32;
        let dy = p.y as i32 - self.y as i32;

        let move_x = dx.signum() * step_x.min(dx.abs() as u32) as i32;
        let move_y = dy.signum() * step_y.min(dy.abs() as u32) as i32;

        self.x = (self.x as i32 + move_x) as u32;
        self.y = (self.y as i32 + move_y) as u32;
    }

    /// Clona la posición
    pub fn clone(&self) -> Self {
        Self {
            x: self.x,
            y: self.y,
        }
    }

    /// Simula un movimiento aleatorio
    /// La posición se mueve en un rango de -10 a 10 en ambas coordenadas
    /// Si la posición se sale de los límites, se ajusta a los mismos . Los límites son 0 y 100

    pub fn simulate(&mut self) {
        let mut rng = rand::thread_rng();

        let x: i32 = (self.x as i32) - rng.gen_range(-10..=10);
        let y: i32 = (self.y as i32) - rng.gen_range(-10..=10);

        self.x = if x < 0 {
            0
        } else if x > 100 {
            100
        } else {
            x
        } as u32;

        self.y = if y < 0 {
            0
        } else if y > 100 {
            100
        } else {
            y
        } as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let pos = Position::new(10, 20);
        assert_eq!(pos.x, 10);
        assert_eq!(pos.y, 20);
    }

    #[test]
    fn test_random() {
        let pos = Position::random();
        assert!(pos.x < 100);
        assert!(pos.y < 100);
    }

    #[test]
    fn test_infinity() {
        let pos = Position::infinity();
        assert_eq!(pos.x, u32::MAX);
        assert_eq!(pos.y, u32::MAX);
    }

    #[test]
    fn test_distance_to() {
        let pos1 = Position::new(10, 20);
        let pos2 = Position::new(15, 30);
        assert_eq!(pos1.distance_to(&pos2), 15); // |15-10| + |30-20| = 5 + 10 = 15

        // Test distance when positions are out of bounds
        let pos3 = Position::new(101, 20);
        assert_eq!(pos1.distance_to(&pos3), u32::MAX);
    }

    #[test]
    fn test_clone() {
        let pos1 = Position::new(10, 20);
        let pos2 = pos1.clone();
        assert_eq!(pos1.x, pos2.x);
        assert_eq!(pos1.y, pos2.y);
    }

    #[test]
    fn test_go_to() {
        let mut pos1 = Position::new(10, 10);
        let pos2 = Position::new(20, 20);
        pos1.go_to(&pos2);

        // After go_to, pos1 should be closer to pos2
        assert!(pos1.x <= 13 && pos1.x >= 10); // max step is 3
        assert!(pos1.y <= 13 && pos1.y >= 10);
    }

    #[test]
    fn test_simulate() {
        let mut pos = Position::new(50, 50);
        pos.simulate();

        // After simulation, position should be within bounds
        assert!(pos.x <= 100);
        assert!(pos.y <= 100);

        // Test boundary conditions
        let mut pos_edge = Position::new(0, 100);
        pos_edge.simulate();
        assert!(pos_edge.x <= 100);
        assert!(pos_edge.y <= 100);
    }
}

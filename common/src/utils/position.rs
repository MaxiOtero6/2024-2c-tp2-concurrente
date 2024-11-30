use std::u32;

use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Position {
    pub x: u32,
    pub y: u32,
}

impl Position {
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    pub fn random() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            x: rng.gen_range(0..100),
            y: rng.gen_range(0..100),
        }
    }

    pub fn infinity() -> Self {
        Self {
            x: u32::MAX,
            y: u32::MAX,
        }
    }

    pub fn distance_to(&self, p: &Position) -> u32 {
        if self.x > 100 || p.x > 100 || self.y > 100 || p.y > 100 {
            return u32::MAX;
        }

        let x: u32 = (p.x as i32 - self.x as i32).abs() as u32;
        let y: u32 = (p.y as i32 - self.y as i32).abs() as u32;

        x + y
    }

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

    pub fn clone(&self) -> Self {
        Self {
            x: self.x,
            y: self.y,
        }
    }

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
    use crate::utils::position::Position;

    // Test the new() constructor
    #[test]
    fn test_new_position() {
        let pos = Position::new(10, 20);
        assert_eq!(pos.x, 10, "x coordinate should be set correctly");
        assert_eq!(pos.y, 20, "y coordinate should be set correctly");
    }

    // Test the random() method
    #[test]
    fn test_random_position() {
        let pos = Position::random();
        assert!(pos.x < 100, "x coordinate should be less than 100");
        assert!(pos.y < 100, "y coordinate should be less than 100");
    }

    // Test the infinity() method
    #[test]
    fn test_infinity_position() {
        let pos = Position::infinity();
        assert_eq!(pos.x, u32::MAX, "x coordinate should be u32::MAX");
        assert_eq!(pos.y, u32::MAX, "y coordinate should be u32::MAX");
    }

    // Test distance_to method
    #[test]
    fn test_distance_calculation() {
        // Test same point
        let pos1 = Position::new(10, 20);
        let pos2 = Position::new(10, 20);
        assert_eq!(
            pos1.distance_to(&pos2),
            0,
            "Distance to same point should be 0"
        );

        // Test different points
        let pos3 = Position::new(5, 10);
        let pos4 = Position::new(15, 25);
        assert_eq!(
            pos3.distance_to(&pos4),
            25,
            "Manhattan distance should be correct"
        );

        // Test reversed points
        assert_eq!(
            pos4.distance_to(&pos3),
            25,
            "Distance calculation should be symmetric"
        );
    }

    // Test clone method
    #[test]
    fn test_clone_method() {
        let original = Position::new(15, 25);
        let cloned = original.clone();

        assert_eq!(original.x, cloned.x, "Cloned x should match original");
        assert_eq!(original.y, cloned.y, "Cloned y should match original");
    }

    // Test simulate method
    #[test]
    fn test_simulate_method() {
        let mut pos = Position::new(50, 50);
        let initial_x = pos.x;
        let initial_y = pos.y;

        pos.simulate();

        // Verify position remains within 0-100 grid
        assert!(pos.x <= 100, "x coordinate should not exceed 100");
        assert!(pos.y <= 100, "y coordinate should not exceed 100");

        // Verify some movement has occurred
        assert!(
            (pos.x as i32 - initial_x as i32).abs() <= 10,
            "x coordinate should change within ±10"
        );
        assert!(
            (pos.y as i32 - initial_y as i32).abs() <= 10,
            "y coordinate should change within ±10"
        );
    }
}

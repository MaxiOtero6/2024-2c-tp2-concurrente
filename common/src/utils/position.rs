use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
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

    pub fn distance_to(self, p: Position) -> u32 {
        let x: u32 = (p.x as i32 - self.x as i32).abs() as u32;
        let y: u32 = (p.y as i32 - self.y as i32).abs() as u32;

        x + y
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

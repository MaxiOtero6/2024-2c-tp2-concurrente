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

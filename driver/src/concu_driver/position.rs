use rand::Rng;

#[derive(Debug)]
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
}

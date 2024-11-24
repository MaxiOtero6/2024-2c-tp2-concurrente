use std::error::Error;

use driver::concu_driver::consts::LOG_LEVEL;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder()
        .filter_level(LOG_LEVEL)
        .init();

    driver::run()
}

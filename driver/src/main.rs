use std::error::Error;

use driver::concu_driver::consts::LOG_LEVEL;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::builder()
        .filter_level(LOG_LEVEL)
        .format_target(false)
        .format_module_path(false)
        // .format_timestamp_micros()
        .format_timestamp(None)
        .init();

    driver::run()
}

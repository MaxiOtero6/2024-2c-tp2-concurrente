use std::error::Error;

use concu_payment::payment::handle_payments;

pub mod concu_payment;

pub fn run() -> Result<(), Box<dyn Error>> {
    handle_payments()
}

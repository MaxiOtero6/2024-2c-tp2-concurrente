use std::error::Error;

use actix::System;
use tokio::join;

use super::{
    central_driver::{CentralDriver, SetPaymentAddr},
    connections_handler::DriverConnectionsHandler,
    payment_connection::PaymentConnection,
};

pub fn drive(id: u32) -> Result<(), Box<dyn Error>> {
    System::new().block_on(connect_all(id))?;

    Ok(())
}

async fn connect_all(id: u32) -> Result<(), Box<dyn Error>> {
    let cdriver = CentralDriver::create_new(id);

    // let connection_with_payment = PaymentConnection::connect(cdriver.clone()).await?;

    // cdriver.send(SetPaymentAddr {
    //     connection_with_payment,
    // }).await?;

    let drivers_conn_task = DriverConnectionsHandler::run(id, cdriver.clone());

    let (drivers_conn_join,) = join!(drivers_conn_task);

    drivers_conn_join.map_err(|e| {
        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        e.to_string()
    })??;

    Ok(())
}

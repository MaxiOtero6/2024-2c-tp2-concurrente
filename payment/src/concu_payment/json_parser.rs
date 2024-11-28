use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub enum PaymentMessages {
    AuthPayment { passenger_id: u32 },
    CollectPayment { driver_id: u32, passenger_id: u32 },
}

#[derive(Serialize)]
pub enum PaymentResponses {
    AuthPayment { passenger_id: u32, response: bool },
    CollectPayment { passenger_id: u32, response: bool },
}

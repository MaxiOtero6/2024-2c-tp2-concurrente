use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum PaymentMessages {
    AuthPayment { id: u32 },
    CollectPayment { id: u32 },
}

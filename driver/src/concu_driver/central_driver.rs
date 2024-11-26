use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, SpawnHandle};

use crate::concu_driver::{driver_connection::SendAll, json_parser::DriverMessages};

use super::{
    consts::ELECTION_TIMEOUT_DURATION, driver_connection::DriverConnection,
    handle_trip::TripHandler, passenger_connection::PassengerConnection,
    payment_connection::PaymentConnection, position::Position,
};

pub struct CentralDriver {
    // Direccion del actor TripHandler
    trip_handler: Addr<TripHandler>,
    // Direccion del actor PassengerConnection
    connection_with_passenger: Option<Addr<PassengerConnection>>,
    // Direccion del actor PaymentConnection
    connection_with_payment: Option<Addr<PaymentConnection>>,
    // Direcciones de los drivers segun su id
    connection_with_drivers: HashMap<u32, Addr<DriverConnection>>, // 0...N
    // Posiciones de los demas drivers segun su id,
    // cobra sentido si este driver es lider
    driver_positions: Arc<Mutex<HashMap<u32, Position>>>,
    // Id del driver lider
    leader_id: Option<u32>,
    // Id del driver
    id: u32,
    // Timeout de la eleccion
    election_timeout: Option<SpawnHandle>,
}

impl Actor for CentralDriver {
    type Context = Context<Self>;
}

impl CentralDriver {
    pub fn create_new(id: u32) -> Addr<Self> {
        CentralDriver::create(|ctx| Self {
            id,
            leader_id: None,
            driver_positions: Arc::new(Mutex::new(HashMap::new())),
            connection_with_drivers: HashMap::new(),
            trip_handler: TripHandler::new(ctx.address()).start(),
            connection_with_payment: None,
            connection_with_passenger: None,
            election_timeout: None,
        })
    }

    fn im_leader(&self) -> bool {
        if let Some(lid) = self.leader_id {
            return self.id == lid;
        }

        false
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct NotifyPositionToLeader {
    pub driver_location: Position,
}

impl Handler<NotifyPositionToLeader> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: NotifyPositionToLeader, ctx: &mut Context<Self>) -> Self::Result {
        if self.leader_id.is_none() {
            return;
        }

        if let Some(lid) = self.leader_id {
            if self.im_leader() {
                ctx.address().do_send(SetDriverPosition {
                    driver_id: self.id,
                    driver_position: msg.driver_location,
                });

                return;
            }

            let parsed_data = serde_json::to_string(&DriverMessages::NotifyPosition {
                driver_id: self.id,
                driver_position: msg.driver_location,
            })
            .inspect_err(|e| log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string()));

            if let Ok(data) = parsed_data {
                match self.connection_with_drivers.get(&lid) {
                    Some(leader) => leader.do_send(SendAll { data }),
                    None => (),
                };
            }
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SetDriverPosition {
    pub driver_id: u32,
    pub driver_position: Position,
}

impl Handler<SetDriverPosition> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: SetDriverPosition, _ctx: &mut Context<Self>) -> Self::Result {
        let lock = self.driver_positions.clone();

        if let Ok(mut wlock) = lock.lock() {
            log::debug!("Driver {} in {:?}", msg.driver_id, msg.driver_position);
            wlock.insert(msg.driver_id, msg.driver_position);
        };
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SetPaymentAddr {
    pub connection_with_payment: Addr<PaymentConnection>,
}

impl Handler<SetPaymentAddr> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: SetPaymentAddr, _ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Connecting with payments service");
        self.connection_with_payment = Some(msg.connection_with_payment);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SetPassengerAddr {
    pub connection_with_passenger: Addr<PassengerConnection>,
}

impl Handler<SetPassengerAddr> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: SetPassengerAddr, _ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Connecting with new passenger");
        self.connection_with_passenger = Some(msg.connection_with_passenger);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct InsertDriverConnection {
    pub id: u32,
    pub addr: Addr<DriverConnection>,
}

impl Handler<InsertDriverConnection> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: InsertDriverConnection, _ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Connecting with driver {}", msg.id);
        self.connection_with_drivers.insert(msg.id, msg.addr);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RemoveDriverConnection {
    pub id: u32,
}

impl Handler<RemoveDriverConnection> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: RemoveDriverConnection, _ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Disconnecting with driver {}", msg.id);
        self.connection_with_drivers.remove(&msg.id);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct StartElection {}

impl Handler<StartElection> for CentralDriver {
    type Result = ();

    fn handle(&mut self, _msg: StartElection, ctx: &mut Context<Self>) -> Self::Result {
        self.leader_id = None;
        let mut higher_processes = false;

        let parsed_data =
            serde_json::to_string(&DriverMessages::Election { sender_id: self.id }).inspect_err(|e| 
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string()),
            );

        // Send election messages to all processes with higher IDs
        for (&id, driver) in &self.connection_with_drivers {
            if id > self.id {
                if let Ok(data) = &parsed_data {
                    driver.do_send(SendAll { data: data.clone() });
                }

                higher_processes = true;
            }
        }

        // If no higher processes, declare Coordinator
        if !higher_processes {
            log::info!("[ELECTION] There is no one bigger than me!");
            ctx.notify(Coordinator { leader_id: self.id });
            
            for (_, driver) in &self.connection_with_drivers {
                let parsed_data =
                    serde_json::to_string(&DriverMessages::Coordinator { leader_id: self.id })
                        .inspect_err(|e| log::error!(
                            "{}:{}, {}",
                            std::file!(),
                            std::line!(),
                            e.to_string()
                        ));

                if let Ok(data) = parsed_data {
                    driver.do_send(SendAll { data });
                }
            }
        } else {
            let leader_id = self.id.clone();
            // Set timeout for responses
            self.election_timeout =
                Some(ctx.run_later(ELECTION_TIMEOUT_DURATION, move |_, ctx| {
                    // Falta notificar a todos la victoria ??
                    log::warn!("[ELECTION] No one answer the election");
                    ctx.notify(Coordinator { leader_id });
                }));
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Election {
    pub sender_id: u32,
}

impl Handler<Election> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: Election, ctx: &mut Context<Self>) -> Self::Result {
        log::debug!(
            "[ELECTION] Driver {} received election message from {}",
            self.id,
            msg.sender_id
        );

        // If this process has higher ID, respond and start new election
        if self.id > msg.sender_id {
            // Send alive message to sender
            if let Some(sender) = self.connection_with_drivers.get(&msg.sender_id) {
                let parsed_data = serde_json::to_string(&DriverMessages::Alive {
                    responder_id: self.id,
                })
                .inspect_err(|e| log::error!(
                    "{}:{}, {}",
                    std::file!(),
                    std::line!(),
                    e.to_string()
                ));

                if let Ok(data) = parsed_data {
                    sender.do_send(SendAll { data });
                }
            }

            // Start new election
            ctx.notify(StartElection {});
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Alive {
    pub responder_id: u32,
}

impl Handler<Alive> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: Alive, ctx: &mut Context<Self>) -> Self::Result {
        log::debug!(
            "[ELECTION] Driver {} received alive message from {}",
            self.id,
            msg.responder_id
        );

        if let Some(timeout) = self.election_timeout {
            ctx.cancel_future(timeout);
            self.election_timeout = None;
        }
        // Cancel election timeout as we received a response
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Coordinator {
    pub leader_id: u32,
}

impl Handler<Coordinator> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: Coordinator, _ctx: &mut Context<Self>) -> Self::Result {
        log::info!("[ELECTION] {} is the new leader", msg.leader_id);

        self.leader_id = Some(msg.leader_id);

        if self.im_leader() {
            log::info!("[ELECTION] Oh!, that is me");
        }
    }
}

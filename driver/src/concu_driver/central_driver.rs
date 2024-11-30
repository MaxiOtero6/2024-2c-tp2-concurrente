use std::{collections::HashMap, sync::Arc};

use actix::{
    dev::ContextFutureSpawner, fut::wrap_future, Actor, Addr, AsyncContext, Context, Handler,
    Message, SpawnHandle,
};
use actix_async_handler::async_handler;
use common::utils::{
    json_parser::{PaymentMessages, TripMessages, TripStatus},
    position::Position,
};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tokio::{sync::Mutex, time::sleep};

use crate::concu_driver::{
    consts::{MAX_DISTANCE, TAKE_TRIP_TIMEOUT_MS},
    driver_connection::{CheckACK, SendAll},
    handle_trip::ForceNotifyPosition,
    json_parser::DriverMessages,
};

use super::{
    consts::ELECTION_TIMEOUT_DURATION, driver_connection::DriverConnection,
    handle_trip::TripHandler, passenger_connection::PassengerConnection,
    payment_connection::PaymentConnection,
};

pub struct CentralDriver {
    // Direccion del actor TripHandler
    trip_handler: Addr<TripHandler>,
    // Direccion del actor PassengerConnection
    passengers: Arc<Mutex<HashMap<u32, Addr<PassengerConnection>>>>,
    // Direcciones de los drivers segun su id
    connection_with_drivers: HashMap<u32, Addr<DriverConnection>>, // 0...N
    // Posiciones de los demas drivers segun su id,
    // cobra sentido si este driver es lider
    driver_positions: HashMap<u32, Position>,
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
            driver_positions: HashMap::new(),
            connection_with_drivers: HashMap::new(),
            trip_handler: TripHandler::new(ctx.address(), id).start(),
            passengers: Arc::new(Mutex::new(HashMap::new())),
            election_timeout: None,
        })
    }

    fn im_leader(&self) -> bool {
        if let Some(lid) = self.leader_id {
            return self.id == lid;
        }

        false
    }

    async fn find_driver(
        driver_positions: &HashMap<u32, Position>,
        id: &u32,
        connection_with_drivers: &HashMap<u32, Addr<DriverConnection>>,
        msg: &FindDriver,
        trip_handler: &Addr<TripHandler>,
        self_addr: &Addr<Self>,
    ) {
        let nearby_drivers = driver_positions
            .clone()
            .into_par_iter()
            .filter(|(_, v)| v.distance_to(&msg.source) <= MAX_DISTANCE)
            .map(|(k, _)| k)
            .collect::<Vec<u32>>();

        log::debug!(
            "[TRIP] Nearby drivers for passenger {}: {:?}",
            msg.passenger_id,
            nearby_drivers
        );

        let parsed_data = serde_json::to_string(&DriverMessages::CanHandleTrip {
            passenger_id: msg.passenger_id,
            passenger_location: msg.source,
            destination: msg.destination,
        })
        .inspect_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        });

        if let Ok(data) = &parsed_data {
            for did in nearby_drivers {
                log::info!(
                    "[TRIP] Asking driver {} if it will take the trip for passenger {}",
                    did,
                    msg.passenger_id
                );
                if did == *id {
                    let res = trip_handler
                        .send(super::handle_trip::CanHandleTrip {
                            passenger_id: msg.passenger_id,
                            passenger_location: msg.source,
                            destination: msg.destination,
                            self_id: *id,
                        })
                        .await
                        .map_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                            e.to_string()
                        });

                    if let Ok(value) = res {
                        if value {
                            log::info!(
                                "[TRIP] Driver {} will take the trip for passenger {}",
                                did,
                                msg.passenger_id
                            );

                            return;
                        }
                    }
                }

                if let Some(driver) = connection_with_drivers.get(&did) {
                    let _ = driver
                        .try_send(SendAll { data: data.clone() })
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        });

                    sleep(TAKE_TRIP_TIMEOUT_MS).await;

                    let res = driver
                        .send(CheckACK {
                            passenger_id: msg.passenger_id,
                        })
                        .await
                        .map_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                            e.to_string()
                        });

                    if let Ok(ack) = res {
                        if let Some(value) = ack {
                            if value {
                                log::info!(
                                    "[TRIP] Driver {} will take the trip for passenger {}",
                                    did,
                                    msg.passenger_id
                                );

                                return;
                            }
                        }
                    }
                }

                log::debug!(
                    "[TRIP] Driver {} can not take the trip or did not answer",
                    did
                )
            }
        }

        let detail = format!("There are no drivers available near your location");

        match self_addr.try_send(ConnectWithPassenger {
            passenger_id: msg.passenger_id,
        }) {
            Err(e) => {
                log::error!("Can not connect with passenger {}", msg.passenger_id);
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
            }
            Ok(_) => {
                let _ = self_addr
                    .try_send(SendTripResponse {
                        status: TripStatus::Error,
                        detail,
                        passenger_id: msg.passenger_id,
                    })
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    });
            }
        }
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
        log::debug!("Driver {} in {:?}", msg.driver_id, msg.driver_position);
        self.driver_positions
            .insert(msg.driver_id, msg.driver_position);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CollectMoneyPassenger {
    pub passenger_id: u32,
}

#[async_handler]
impl Handler<CollectMoneyPassenger> for CentralDriver {
    type Result = ();

    async fn handle(
        &mut self,
        msg: CollectMoneyPassenger,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        let driver_id = self.id.clone();
        let self_addr = _ctx.address().clone();

        let connection_with_payment = PaymentConnection::connect(self_addr).await;

        if let Ok(addr) = connection_with_payment {
            let parsed_data = serde_json::to_string(&PaymentMessages::CollectPayment {
                driver_id,
                passenger_id: msg.passenger_id,
            })
            .inspect_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            });

            if let Ok(data) = parsed_data {
                let _ = addr
                    .try_send(super::payment_connection::SendAll { data })
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    });
            }
        }

        ()
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct CheckPaymentResponse {
    pub passenger_id: u32,
    pub response: bool,
}

impl Handler<CheckPaymentResponse> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: CheckPaymentResponse, _ctx: &mut Context<Self>) -> Self::Result {
        match msg.response {
            true => log::info!("Passenger {} paid for the trip!", msg.passenger_id),
            _ => log::warn!(
                "Passenger {} did not pay for the trip!!, call the police!",
                msg.passenger_id
            ),
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RemovePassengerConnection {
    pub id: u32,
}

impl Handler<RemovePassengerConnection> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: RemovePassengerConnection, ctx: &mut Context<Self>) -> Self::Result {
        let passenger = self.passengers.clone();

        wrap_future::<_, Self>(async move {
            let mut lock = passenger.lock().await;

            if let Some(_) = (*lock).remove(&msg.id) {
                log::info!("Disconnecting with passenger {}", msg.id);
            }
        })
        .spawn(ctx);
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
        if let Some(_) = self.connection_with_drivers.remove(&msg.id) {
            log::info!("Disconnecting with driver {}", msg.id);
        }
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

        let parsed_data = serde_json::to_string(&DriverMessages::Election { sender_id: self.id })
            .inspect_err(|e| log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string()));

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
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                        });

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
                .inspect_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string())
                });

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

        let _ = self
            .trip_handler
            .try_send(ForceNotifyPosition {})
            .inspect_err(|e| log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string()));
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct RedirectNewTrip {
    pub passenger_id: u32,
    pub source: Position,
    pub destination: Position,
}

impl Handler<RedirectNewTrip> for CentralDriver {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: RedirectNewTrip, ctx: &mut Context<Self>) -> Self::Result {
        log::debug!(
            "[TRIP] Redirect to leader a trip for passenger {}",
            msg.passenger_id
        );

        if let Some(lid) = &self.leader_id {
            if self.im_leader() {
                ctx.address()
                    .try_send(FindDriver {
                        passenger_id: msg.passenger_id,
                        source: msg.source,
                        destination: msg.destination,
                    })
                    .map_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        e.to_string()
                    })?;
            } else {
                let leader_addr = self.connection_with_drivers.get(lid);

                if let Some(laddr) = leader_addr {
                    let data = serde_json::to_string(&DriverMessages::TripRequest {
                        passenger_id: msg.passenger_id,
                        passenger_location: msg.source,
                        destination: msg.destination,
                    })
                    .map_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        e.to_string()
                    })?;

                    laddr.try_send(SendAll { data }).map_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        e.to_string()
                    })?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct FindDriver {
    pub passenger_id: u32,
    pub source: Position,
    pub destination: Position,
}

impl Handler<FindDriver> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: FindDriver, ctx: &mut Context<Self>) -> Self::Result {
        log::debug!("[TRIP] Finding a driver for passenger {}", msg.passenger_id);

        let driver_positions = self.driver_positions.clone();
        let id = self.id.clone();
        let connection_with_drivers = self.connection_with_drivers.clone();
        let trip_handler = self.trip_handler.clone();
        let self_addr = ctx.address().clone();

        wrap_future::<_, Self>(async move {
            CentralDriver::find_driver(
                &driver_positions,
                &id,
                &connection_with_drivers,
                &msg,
                &trip_handler,
                &self_addr,
            )
            .await;
        })
        .spawn(ctx);

        ()
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct CanHandleTrip {
    pub passenger_id: u32,
    pub source: Position,
    pub destination: Position,
}

impl Handler<CanHandleTrip> for CentralDriver {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: CanHandleTrip, ctx: &mut Context<Self>) -> Self::Result {
        let leader_id = self.leader_id.clone();
        let connection_with_drivers = self.connection_with_drivers.clone();
        let trip_handler = self.trip_handler.clone();
        let self_id = self.id.clone();

        wrap_future::<_, Self>(async move {
            let res = trip_handler
                .send(super::handle_trip::CanHandleTrip {
                    passenger_id: msg.passenger_id,
                    passenger_location: msg.source,
                    destination: msg.destination,
                    self_id,
                })
                .await
                .inspect_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                })
                .unwrap_or(false);

            if let Some(lid) = &leader_id {
                if let Some(leader) = connection_with_drivers.get(lid) {
                    let parsed_data = serde_json::to_string(&DriverMessages::CanHandleTripACK {
                        response: res,
                        passenger_id: msg.passenger_id,
                    })
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    });

                    if let Ok(data) = parsed_data {
                        let _ = leader.try_send(SendAll { data }).inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        });
                    }
                }
            }
        })
        .spawn(ctx);

        Ok(())
    }
}

#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct ConnectWithPassenger {
    pub passenger_id: u32,
}

#[async_handler]
impl Handler<ConnectWithPassenger> for CentralDriver {
    type Result = Result<(), String>;

    async fn handle(
        &mut self,
        msg: ConnectWithPassenger,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        let passenger = self.passengers.clone();
        let self_addr = _ctx.address();

        let passenger_addr =
            PassengerConnection::connect(self_addr.clone(), msg.passenger_id).await;

        let mut lock = passenger.lock_owned().await;

        match passenger_addr {
            Ok(addr) => {
                log::info!("Connecting with passenger {}", msg.passenger_id);

                (*lock).insert(msg.passenger_id, addr);

                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendTripResponse {
    pub status: TripStatus,
    pub detail: String,
    pub passenger_id: u32,
}

impl Handler<SendTripResponse> for CentralDriver {
    type Result = ();

    fn handle(&mut self, msg: SendTripResponse, ctx: &mut Context<Self>) -> Self::Result {
        let parsed_data = serde_json::to_string(&TripMessages::TripResponse {
            status: msg.status,
            detail: msg.detail.clone(),
        })
        .inspect_err(|e| {
            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
        });

        let passenger = self.passengers.clone();

        wrap_future::<_, Self>(async move {
            let lock = passenger.lock().await;

            if let Ok(data) = parsed_data {
                if let Some(paddr) = (*lock).get(&msg.passenger_id) {
                    let _ = paddr
                        .try_send(super::passenger_connection::SendAll { data })
                        .inspect_err(|e| {
                            log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                        });
                }
            }
        })
        .spawn(ctx);
    }
}

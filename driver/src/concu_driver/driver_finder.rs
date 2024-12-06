use std::collections::{HashMap, VecDeque};

use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, SpawnHandle};
use actix_async_handler::async_handler;
use common::utils::{json_parser::TripStatus, position::Position};
use rayon::{
    iter::{IntoParallelIterator, ParallelIterator},
    slice::ParallelSliceMut,
};

use crate::concu_driver::{
    central_driver::{CanHandleTrip, ConnectWithPassenger, RemoveDriverFinder, SendTripResponse},
    consts::TAKE_TRIP_TIMEOUT_MS,
};

use super::{central_driver::CentralDriver, consts::MAX_DISTANCE};

pub struct DriverFinder {
    /// Direccion del actor CentralDriver
    central_driver: Addr<CentralDriver>,
    /// Timeout para la recepcion de confirmacion de un driver
    driver_ack_timeout: Option<SpawnHandle>,
    /// Id del pasajero
    passenger_id: Option<u32>,
    /// Posicion inicial del pasajero
    source: Position,
    /// Posicion destino del pasajero
    destination: Position,
    /// Conductores cercanos a la posicion inicial del pasajero
    nearby_drivers: VecDeque<u32>,
}

impl Actor for DriverFinder {
    type Context = Context<Self>;

    /// Al momento de iniciar el actor, calcula los conductores cercanos y se notifica el mensaje AskDrivers
    fn started(&mut self, ctx: &mut Self::Context) {
        if let Some(pid) = self.passenger_id {
            log::debug!(
                "[TRIP] Nearby drivers for passenger {}: {:?}",
                pid,
                self.nearby_drivers
            );

            ctx.notify(AskDrivers {
                previous_driver: None,
            });
        }
    }
}

impl DriverFinder {
    /// Crea un nuevo struct DriverFinder
    pub fn new(
        central_driver: Addr<CentralDriver>,
        passenger_id: u32,
        source: Position,
        destination: Position,
        driver_positions: HashMap<u32, Position>,
    ) -> Self {
        Self {
            central_driver,
            driver_ack_timeout: None,
            passenger_id: Some(passenger_id),
            source,
            destination,
            nearby_drivers: Self::filter_nearby_drivers(&source, &driver_positions),
        }
    }

    /// Filtra los drivers cercanos a una posicion dada.
    fn filter_nearby_drivers(
        source: &Position,
        driver_positions: &HashMap<u32, Position>,
    ) -> VecDeque<u32> {
        let mut distances = driver_positions
            .clone()
            .into_par_iter()
            .map(|(k, v)| (k, v.distance_to(&source)))
            .filter(|(_, v)| *v <= MAX_DISTANCE)
            .collect::<Vec<(u32, u32)>>();

        distances.par_sort_by(|(_, a), (_, b)| a.cmp(&b));

        let nearby_drivers = distances
            .into_par_iter()
            .map(|(k, _)| k)
            .collect::<VecDeque<u32>>();

        nearby_drivers
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct AskDrivers {
    /// Id del conductor previamente consultado
    previous_driver: Option<u32>,
}

impl Handler<AskDrivers> for DriverFinder {
    type Result = ();

    /// Consulta uno por uno a los conductores desde el mas cercano al mas lejano (en rangod), para ver si
    /// quieren / pueden tomar el viaje.
    /// Inicia el timeout driver_ack_timeout de TAKE_TRIP_TIMEOUT_MS milisegundos que vuelve a notificar este mensaje.
    fn handle(&mut self, msg: AskDrivers, ctx: &mut Context<Self>) -> Self::Result {
        if self.passenger_id.is_none() {
            return;
        }

        if let Some(prevd) = msg.previous_driver {
            log::debug!(
                "[TRIP] Driver {} can not take the trip or did not answer",
                prevd
            )
        }

        let pid = self.passenger_id.unwrap();

        let poped_id = self.nearby_drivers.pop_front();

        if let None = poped_id {
            ctx.notify(NoDrivers { passenger_id: pid });
            return;
        }

        let did = poped_id.unwrap();

        log::info!(
            "[TRIP] Asking driver {} if it will take the trip for passenger {}",
            did,
            pid
        );

        let _ = self
            .central_driver
            .try_send(CanHandleTrip {
                passenger_id: pid,
                source: self.source,
                destination: self.destination,
                driver_id: did,
            })
            .inspect_err(|e| {
                log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
            });

        self.driver_ack_timeout = Some(ctx.notify_later(
            AskDrivers {
                previous_driver: Some(did),
            },
            TAKE_TRIP_TIMEOUT_MS,
        ));
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct DriverACK {
    /// Id del conductor que envia el ACK
    pub driver_id: u32,
    /// Valor del ACK
    pub response: bool,
}

impl Handler<DriverACK> for DriverFinder {
    type Result = ();

    /// Recepcion de respuesta de un driver. Cancela el timeout driver_ack_timeout.
    /// En caso afirmativo, se deja de buscar un conductor.
    /// En caso negativo, el actor se notifica el mensaje AskDrivers para seguir consultando a los demas.
    fn handle(&mut self, msg: DriverACK, ctx: &mut Context<Self>) -> Self::Result {
        if let Some(fut) = self.driver_ack_timeout.take() {
            ctx.cancel_future(fut);
        }

        if !msg.response {
            ctx.notify(AskDrivers {
                previous_driver: Some(msg.driver_id),
            });
            return;
        }

        if self.passenger_id.is_none() {
            return;
        }

        let pid = self.passenger_id.take().unwrap();

        log::info!(
            "[TRIP] Driver {} will take the trip for passenger {}",
            msg.driver_id,
            pid
        );

        self.central_driver
            .do_send(RemoveDriverFinder { passenger_id: pid });
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct NoDrivers {
    /// Id del pasajero
    passenger_id: u32,
}

#[async_handler]
impl Handler<NoDrivers> for DriverFinder {
    type Result = ();

    /// Notifica al central driver que el viaje para el pasajero no tiene conductores libres cercanos.
    async fn handle(&mut self, msg: NoDrivers, _ctx: &mut Context<Self>) -> Self::Result {
        let pid = self.passenger_id.take().unwrap();

        let cd_addr = self.central_driver.clone();

        let res = async move {
            cd_addr
                .send(ConnectWithPassenger {
                    passenger_id: msg.passenger_id,
                })
                .await
                .map_err(|e| {
                    log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    e.to_string()
                })
        }
        .await;

        match res {
            Ok(Ok(_)) => {
                let detail = format!("There are no drivers available near your location");

                log::info!("There are no drivers near passenger {}", msg.passenger_id);

                let _ = self
                    .central_driver
                    .try_send(SendTripResponse {
                        status: TripStatus::Error,
                        detail,
                        passenger_id: msg.passenger_id,
                    })
                    .inspect_err(|e| {
                        log::error!("{}:{}, {}", std::file!(), std::line!(), e.to_string());
                    });
            }
            _ => (),
        }

        self.passenger_id = None;

        self.central_driver
            .do_send(RemoveDriverFinder { passenger_id: pid });
    }
}

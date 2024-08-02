use std::{collections::BTreeMap, net::SocketAddr, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use axum::{extract::State, handler::HandlerWithoutStateExt, response::Redirect, routing::get, Json, Router};
use dashmap::DashMap;
use quark_im::{
    abstracts::{PeerInfo, QuarkIMHook, Service, ServiceMode},
    app_error,
    quark::QuarkIM,
    quic::{create_client, create_server},
    services::{ReferralService, SpeedReportService, SpeedTestService},
    tasks::RoutingQueryTask,
};
use s2n_quic::stream::BidirectionalStream;
use tracing::Level;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_file(true).with_line_number(true).with_max_level(Level::INFO).init();

    let quark_server_port = dotenvy::var("QUARK_SERVER_PORT").unwrap_or("0".into()).parse()?;

    let client = create_client(SocketAddr::from(([0, 0, 0, 0], 0))).await?;
    let server = create_server(SocketAddr::from(([0, 0, 0, 0], quark_server_port))).await?;

    let server_port = server.local_addr()?.port();

    let mut local_info = PeerInfo::new();
    local_info.endpoints.push(SocketAddr::from(([127, 0, 0, 1], server_port)));

    let state = QuarkState {
        speeds: Arc::new(DashMap::new()),
        speed_report: Arc::new(DashMap::new()),
        relay_paths: Arc::new(DashMap::new()),
    };

    let im = QuarkIM::new(IMHook::new(state.clone()), client, server).await?;

    tokio::spawn(RoutingQueryTask::new(im.peer_id, state.speeds.clone(), state.speed_report.clone(), state.relay_paths.clone()).future());

    let routes = root_route(im.clone(), state.clone());
    let server_addr = SocketAddr::from(([0, 0, 0, 0], server_port + 1));
    let tcp_listener = tokio::net::TcpListener::bind(&server_addr).await?;
    axum::serve(tcp_listener, routes.into_make_service()).await?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct IMHook {
    state: QuarkState,
}

impl IMHook {
    pub fn new(state: QuarkState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl QuarkIMHook for IMHook {
    fn both_sides(&self, im: &QuarkIM, peer_id: Uuid) {
        im.start_service_task(peer_id, ReferralService::NAME.into());
        im.start_service_task(peer_id, SpeedTestService::NAME.into());
        im.start_service_task(peer_id, SpeedReportService::NAME.into());
    }

    async fn handle(&self, im: &QuarkIM, stream: BidirectionalStream, peer_id: Uuid, name: String, mode: ServiceMode) -> Result<()> {
        let state = self.state.clone();

        match (name.as_str(), mode) {
            (ReferralService::NAME, ServiceMode::Start) => ReferralService::new(im.clone()).start(stream).await?,
            (ReferralService::NAME, ServiceMode::Handle) => ReferralService::new(im.clone()).handle(stream).await?,
            (SpeedTestService::NAME, ServiceMode::Start) => SpeedTestService::new(peer_id, state.speeds).start(stream).await?,
            (SpeedTestService::NAME, ServiceMode::Handle) => SpeedTestService::new(peer_id, state.speeds).handle(stream).await?,
            (SpeedReportService::NAME, ServiceMode::Start) => SpeedReportService::new(peer_id, state.speeds, state.speed_report).start(stream).await?,
            (SpeedReportService::NAME, ServiceMode::Handle) => SpeedReportService::new(peer_id, state.speeds, state.speed_report).handle(stream).await?,
            _ => {
                tracing::warn!(name, ?mode, "failed to find service")
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct QuarkState {
    speeds: Arc<DashMap<Uuid, u64>>,
    speed_report: Arc<DashMap<Uuid, BTreeMap<Uuid, u64>>>,
    relay_paths: Arc<DashMap<Uuid, (Vec<Uuid>, u64)>>,
}

fn root_route(im: QuarkIM, state: QuarkState) -> Router {
    Router::new()
        .route_service("/", Redirect::temporary("/routing").into_service())
        .route("/routing", get(get_routing))
        .with_state(im)
        .route("/speeds", get(get_speeds))
        .route("/speed_report", get(get_speed_report))
        .with_state(state)
}

async fn get_routing(State(state): State<QuarkIM>) -> app_error::Result<Json<BTreeMap<Uuid, PeerInfo>>> {
    Ok(Json(
        state
            .peers
            .iter()
            .map(|peer| {
                let peer_id = peer.key().clone();
                let info = peer.value().clone();
                (peer_id, info)
            })
            .collect::<BTreeMap<Uuid, PeerInfo>>(),
    ))
}

async fn get_speeds(State(state): State<QuarkState>) -> app_error::Result<Json<BTreeMap<Uuid, u64>>> {
    let mut speeds = BTreeMap::new();
    for item in state.speeds.iter() {
        speeds.insert(*item.key(), *item.value());
    }
    Ok(Json(speeds))
}

async fn get_speed_report(State(state): State<QuarkState>) -> app_error::Result<Json<BTreeMap<Uuid, BTreeMap<Uuid, u64>>>> {
    let mut report = BTreeMap::new();
    for item in state.speed_report.iter() {
        report.insert(*item.key(), item.value().clone());
    }
    Ok(Json(report))
}

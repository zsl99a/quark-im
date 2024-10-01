use std::{collections::BTreeMap, net::SocketAddr, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use axum::{extract::State, handler::HandlerWithoutStateExt, response::Redirect, routing::get, Json, Router};
use dashmap::DashMap;
use quark_im::{
    abstracts::{PeerInfo, QuarkIMHook, Service, ServiceMode},
    app_error,
    quic::{create_client, create_server},
    services::{ReferralService, SpeedReportService, SpeedTestService},
    tasks::RoutingQueryTask,
    QuarkIM,
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

    let hook = IMHook::new();
    let im = QuarkIM::new(client, local_info, hook.clone());
    im.server_task(server);

    if let Ok(endpoint) = dotenvy::var("QUARK_CONNECT_ENDPOINT").unwrap_or_default().parse::<SocketAddr>() {
        im.connect(endpoint).await?;
    }

    tokio::spawn(RoutingQueryTask::new(im.peer_id, hook.speed_report.clone(), hook.relay_paths.clone()).future());

    let routes = root_route(im.clone(), hook.clone());
    let server_addr = SocketAddr::from(([0, 0, 0, 0], server_port + 1));
    let tcp_listener = tokio::net::TcpListener::bind(&server_addr).await?;
    axum::serve(tcp_listener, routes.into_make_service()).await?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct IMHook {
    speed_report: Arc<DashMap<Uuid, BTreeMap<Uuid, u64>>>,
    relay_paths: Arc<DashMap<Uuid, (Vec<Uuid>, u64)>>,
}

impl Default for IMHook {
    fn default() -> Self {
        Self::new()
    }
}

impl IMHook {
    pub fn new() -> Self {
        Self {
            speed_report: Arc::new(DashMap::new()),
            relay_paths: Arc::new(DashMap::new()),
        }
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
        let im = im.clone();
        let hook = self.clone();

        match (name.as_str(), mode) {
            (ReferralService::NAME, ServiceMode::Client) => ReferralService::new(im).client(stream).await,
            (ReferralService::NAME, ServiceMode::Server) => ReferralService::new(im).server(stream).await,
            (SpeedTestService::NAME, ServiceMode::Client) => SpeedTestService::new(im, peer_id, hook.speed_report).client(stream).await,
            (SpeedTestService::NAME, ServiceMode::Server) => SpeedTestService::new(im, peer_id, hook.speed_report).server(stream).await,
            (SpeedReportService::NAME, ServiceMode::Client) => SpeedReportService::new(im, peer_id, hook.speed_report).client(stream).await,
            (SpeedReportService::NAME, ServiceMode::Server) => SpeedReportService::new(im, peer_id, hook.speed_report).server(stream).await,
            _ => anyhow::bail!("failed to find service"),
        }
    }
}

fn root_route(im: QuarkIM, state: IMHook) -> Router {
    Router::new()
        .route_service("/", Redirect::temporary("/routing").into_service())
        .route("/routing", get(get_routing))
        .with_state(im)
        .route("/speed_report", get(get_speed_report))
        .route("/relay_paths", get(get_relay_paths))
        .with_state(state)
}

async fn get_routing(State(state): State<QuarkIM>) -> app_error::Result<Json<BTreeMap<Uuid, PeerInfo>>> {
    Ok(Json(
        state
            .peers
            .iter()
            .map(|peer| {
                let peer_id = *peer.key();
                let info = peer.value().clone();
                (peer_id, info)
            })
            .collect::<BTreeMap<Uuid, PeerInfo>>(),
    ))
}

async fn get_speed_report(State(state): State<IMHook>) -> app_error::Result<Json<BTreeMap<Uuid, BTreeMap<Uuid, u64>>>> {
    let mut report = BTreeMap::new();
    for item in state.speed_report.iter() {
        report.insert(*item.key(), item.value().clone());
    }
    Ok(Json(report))
}

async fn get_relay_paths(State(state): State<IMHook>) -> app_error::Result<Json<BTreeMap<Uuid, (Vec<Uuid>, u64)>>> {
    let mut report = BTreeMap::new();
    for item in state.relay_paths.iter() {
        report.insert(*item.key(), item.value().clone());
    }
    Ok(Json(report))
}

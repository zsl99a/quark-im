use std::{collections::BTreeMap, net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::{extract::State, routing::get, Json, Router};
use dashmap::DashMap;
use quark_im::{
    app_error,
    quic::{create_client, create_server},
    routing::{PeerInfo, Routing, RoutingStore},
    service::Service,
    services::{ReferralService, SpeedReportService, SpeedTestService},
    starting::{connection_starting, session_online, stream_starting, ServiceMode},
    tasks::RoutingQueryTask,
};
use tokio::sync::mpsc;
use tracing::Level;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_file(true).with_line_number(true).with_max_level(Level::INFO).init();

    let quark_server_port = dotenvy::var("QUARK_SERVER_PORT").unwrap_or("0".into()).parse()?;
    let quark_connect_endpoint = dotenvy::var("QUARK_CONNECT_ENDPOINT").unwrap_or_default().parse();

    let client = create_client(SocketAddr::from(([0, 0, 0, 0], 0))).await?;
    let server = create_server(SocketAddr::from(([0, 0, 0, 0], quark_server_port))).await?;

    let server_port = server.local_addr()?.port();

    let mut local_info = PeerInfo::new();
    local_info.endpoints.push(SocketAddr::from(([127, 0, 0, 1], server_port)));

    let routing = Routing::new(local_info);

    let (new_conn_tx, new_conn_rx) = mpsc::channel(32);
    if let Ok(connect_endpoint) = quark_connect_endpoint {
        new_conn_tx.send(connect_endpoint).await?;
    }
    let state = QuarkState {
        routing,
        new_conn_tx,
        speeds: Arc::new(DashMap::new()),
        speed_report: Arc::new(DashMap::new()),
        relay_paths: Arc::new(DashMap::new()),
    };

    tokio::spawn(
        RoutingQueryTask::new(
            state.routing.lock().await.peer_id,
            state.speeds.clone(),
            state.speed_report.clone(),
            state.relay_paths.clone(),
        )
        .future(),
    );

    let routes = Router::new()
        .route("/routing", get(get_routing))
        .route("/speeds", get(get_speeds))
        .route("/speed_report", get(get_speed_report))
        .with_state(state.clone());
    let server_addr = SocketAddr::from(([0, 0, 0, 0], server_port + 1));
    let tcp_listener = tokio::net::TcpListener::bind(&server_addr).await?;
    tokio::spawn(async move { axum::serve(tcp_listener, routes.into_make_service()).await });

    connection_starting(new_conn_rx, client, server, move |bi_stream, connection| {
        let state = state.clone();

        session_online(bi_stream, state.routing.clone(), |peer_id| async move {
            let (new_stream_tx, new_stream_rx) = mpsc::channel::<String>(32);

            new_stream_tx.send(ReferralService::NAME.into()).await?;
            new_stream_tx.send(SpeedTestService::NAME.into()).await?;
            new_stream_tx.send(SpeedReportService::NAME.into()).await?;

            stream_starting(new_stream_rx, connection, move |stream, service_name, service_mode| async move {
                match (service_name.as_str(), service_mode) {
                    (ReferralService::NAME, ServiceMode::Start) => ReferralService::new(state.routing, state.new_conn_tx).start(stream).await?,
                    (ReferralService::NAME, ServiceMode::Handle) => ReferralService::new(state.routing, state.new_conn_tx).handle(stream).await?,
                    (SpeedTestService::NAME, ServiceMode::Start) => SpeedTestService::new(peer_id, state.speeds).start(stream).await?,
                    (SpeedTestService::NAME, ServiceMode::Handle) => SpeedTestService::new(peer_id, state.speeds).handle(stream).await?,
                    (SpeedReportService::NAME, ServiceMode::Start) => SpeedReportService::new(peer_id, state.speeds, state.speed_report).start(stream).await?,
                    (SpeedReportService::NAME, ServiceMode::Handle) => SpeedReportService::new(peer_id, state.speeds, state.speed_report).handle(stream).await?,
                    _ => {
                        tracing::warn!(service_name, ?service_mode, "failed to find service")
                    }
                }
                Ok(())
            })
            .await
        })
    })
    .await
}

#[derive(Debug, Clone)]
pub struct QuarkState {
    routing: Routing,
    new_conn_tx: mpsc::Sender<SocketAddr>,
    speeds: Arc<DashMap<Uuid, u64>>,
    speed_report: Arc<DashMap<Uuid, BTreeMap<Uuid, u64>>>,
    relay_paths: Arc<DashMap<Uuid, (Vec<Uuid>, u64)>>,
}

async fn get_routing(State(state): State<QuarkState>) -> app_error::Result<Json<RoutingStore>> {
    Ok(Json(state.routing.lock().await.clone()))
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

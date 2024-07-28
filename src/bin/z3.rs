use std::{net::SocketAddr, time::Instant};

use anyhow::Result;
use axum::{extract::State, routing::get, Json, Router};
use futures::{SinkExt, StreamExt};
use quark_im::{
    app_error,
    framed_stream::FramedStream,
    io_stream::IoStream,
    message_pack::MessagePack,
    negotiator::Negotiator,
    quic::{create_client, create_server},
    routing::{PeerInfo, Routing, RoutingStore},
    starting::{client_starting, server_starting},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
};
use tracing::Level;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_file(true)
        .with_line_number(true)
        .with_max_level(Level::INFO)
        .init();

    let quark_server_port = dotenvy::var("QUARK_SERVER_PORT").unwrap_or("0".into()).parse()?;
    let quark_connect_endpoint = dotenvy::var("QUARK_CONNECT_ENDPOINT").unwrap_or_default().parse();

    let client = create_client(SocketAddr::from(([0, 0, 0, 0], 0))).await?;
    let server = create_server(SocketAddr::from(([0, 0, 0, 0], quark_server_port))).await?;

    let server_port = server.local_addr()?.port();

    let mut local_info = PeerInfo::new();
    local_info.endpoints.push(SocketAddr::from(([127, 0, 0, 1], server_port)));

    let routing = Routing::new(local_info);

    let (tx, rx) = mpsc::channel(128);
    if let Ok(connect_endpoint) = quark_connect_endpoint {
        tx.send(connect_endpoint).await?;
    }

    let routes = Router::new().route("/", get(get_routing)).with_state(routing.clone());
    let server_addr = SocketAddr::from(([0, 0, 0, 0], server_port + 1));
    let tcp_listener = tokio::net::TcpListener::bind(&server_addr).await?;

    tokio::select! {
        _ = client_starting(rx, client, routing.clone(), start_service) => {}
        _ = server_starting(server, routing.clone(), start_service) => {}
        _ = axum::serve(tcp_listener, routes.into_make_service()) => {}
    }

    Ok(())
}

async fn get_routing(State(state): State<Routing>) -> app_error::Result<Json<RoutingStore>> {
    Ok(Json(state.lock().await.clone()))
}

async fn start_service<I>(stream: I, _routing: Routing, service: String) -> Result<()>
where
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    match service.as_str() {
        "speed_test" => speed_test_service(stream).await?,
        _ => {}
    }

    Ok(())
}

async fn _speed_test_client<I>(mut stream: I) -> Result<()>
where
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    let mut negotiator = Negotiator::new(&mut stream);
    negotiator.send("speed_test".to_string()).await?;
    let _ = negotiator.recv().await?;
    drop(negotiator);

    let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<String, String>::default());

    let ins = Instant::now();

    let msg_count = 1000;

    for i in 0..msg_count {
        stream.send(format!("Hello World {}", i)).await?;
    }

    let mut count: i64 = 0;

    while let Some(Ok(message)) = stream.next().await {
        count += 1;
        if count == msg_count {
            tracing::info!(elapsed = ?ins.elapsed(), message)
        }
    }

    Ok(())
}

async fn speed_test_service<I>(stream: I) -> Result<()>
where
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<String, String>::default());

    while let Some(Ok(message)) = stream.next().await {
        stream.send(message).await?;
    }

    Ok(())
}

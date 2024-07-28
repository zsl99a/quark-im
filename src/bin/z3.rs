use std::net::SocketAddr;

use anyhow::Result;
use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use quark_im::{
    app_error,
    framed_stream::FramedStream,
    io_stream::IoStream,
    message_pack::MessagePack,
    negotiator::Negotiator,
    quic::{create_client, create_server},
    routing::{PeerInfo, Routing, RoutingStore},
    starting::{session_online, worker_starting},
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
    tokio::spawn(async move { axum::serve(tcp_listener, routes.into_make_service()).await });

    worker_starting(rx, client, server, move |stream, connection| {
        let routing = routing.clone();

        session_online(stream, routing.clone(), async move {
            let (mut handle, mut acceptor) = connection.split();

            tokio::spawn(async move {
                let stream = handle.open_bidirectional_stream().await?;
                speed_test_client(stream).await
            });

            // service
            while let Ok(Some(mut stream)) = acceptor.accept_bidirectional_stream().await {
                let routing = routing.clone();
                tokio::spawn(async move {
                    // negotiator
                    let mut negotiator = Negotiator::<String, _>::new(&mut stream);
                    let service = negotiator.recv().await?;

                    // service
                    start_service(stream, routing, service).await
                });
            }
        })
    })
    .await;

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

async fn speed_test_client<I>(mut stream: I) -> Result<()>
where
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    let mut negotiator = Negotiator::new(&mut stream);
    negotiator.send("speed_test".to_string()).await?;

    let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<DateTime<Utc>, DateTime<Utc>>::default());

    stream.send(Utc::now()).await?;
    stream.send(Utc::now()).await?;
    stream.send(Utc::now()).await?;

    while let Some(Ok(message)) = stream.next().await {
        tracing::info!(?message);
    }
    Result::<()>::Ok(())
}

async fn speed_test_service<I>(stream: I) -> Result<()>
where
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<DateTime<Utc>, DateTime<Utc>>::default());

    while let Some(Ok(_message)) = stream.next().await {
        stream.send(Utc::now()).await?;
    }

    Ok(())
}

use std::net::SocketAddr;

use anyhow::Result;
use axum::{extract::State, routing::get, Json, Router};
use quark_im::{
    app_error,
    quic::{create_client, create_server},
    routing::{PeerInfo, Routing, RoutingStore},
    starting::{connection_starting, session_online, stream_starting, ServiceMode},
};
use tokio::sync::mpsc;
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

    let (new_connection_tx, new_connection_rx) = mpsc::channel(32);
    if let Ok(connect_endpoint) = quark_connect_endpoint {
        new_connection_tx.send(connect_endpoint).await?;
    }

    let routes = Router::new().route("/", get(get_routing)).with_state(routing.clone());
    let server_addr = SocketAddr::from(([0, 0, 0, 0], server_port + 1));
    let tcp_listener = tokio::net::TcpListener::bind(&server_addr).await?;
    tokio::spawn(async move { axum::serve(tcp_listener, routes.into_make_service()).await });

    connection_starting(new_connection_rx, client, server, move |stream, connection| {
        let new_connection_tx = new_connection_tx.clone();
        let routing = routing.clone();

        session_online(stream, routing.clone(), async move {
            let (new_stream_tx, new_stream_rx) = mpsc::channel::<String>(32);

            new_stream_tx.send(referral_service::NAME.into()).await?;
            new_stream_tx.send(speed_test_service::NAME.into()).await?;

            stream_starting(new_stream_rx, connection, move |stream, service_name, service_mode| {
                let routing = routing.clone();
                let new_connection_tx = new_connection_tx.clone();

                async move {
                    match (service_name.as_str(), service_mode) {
                        (referral_service::NAME, ServiceMode::Start) => referral_service::start(stream, routing.clone(), new_connection_tx.clone()).await?,
                        (referral_service::NAME, ServiceMode::Handle) => referral_service::handle(stream, routing.clone()).await?,
                        (speed_test_service::NAME, ServiceMode::Start) => speed_test_service::start(stream).await?,
                        (speed_test_service::NAME, ServiceMode::Handle) => speed_test_service::handle(stream).await?,
                        _ => {
                            tracing::warn!(service_name, ?service_mode, "failed to find service")
                        }
                    }
                    Ok(())
                }
            })
            .await
        })
    })
    .await;

    Ok(())
}

async fn get_routing(State(state): State<Routing>) -> app_error::Result<Json<RoutingStore>> {
    Ok(Json(state.lock().await.clone()))
}

mod referral_service {
    use std::net::SocketAddr;

    use anyhow::Result;
    use futures::{SinkExt, StreamExt};
    use quark_im::{
        framed_stream::FramedStream,
        io_stream::IoStream,
        message_pack::MessagePack,
        negotiator::Negotiator,
        routing::{PeerInfo, Routing},
    };
    use tokio::{
        io::{AsyncRead, AsyncWrite},
        sync::mpsc,
    };

    pub const NAME: &'static str = "ReferralService";

    pub async fn start<I>(mut stream: I, routing: Routing, sender: mpsc::Sender<SocketAddr>) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        let mut negotiator = Negotiator::new(&mut stream);
        negotiator.send(NAME.to_string()).await?;

        let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<PeerInfo, ()>::default());

        while let Some(Ok(peer_info)) = stream.next().await {
            if !routing.lock().await.peers.contains_key(&peer_info.peer_id) {
                for endpoint in peer_info.endpoints {
                    sender.send(endpoint).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn handle<I>(stream: I, routing: Routing) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<(), PeerInfo>::default());

        for peer_info in routing.lock().await.peers.values().cloned() {
            stream.send(peer_info).await?;
        }

        while let Some(Ok(_message)) = stream.next().await {}

        Ok(())
    }
}

mod speed_test_service {
    use anyhow::Result;
    use chrono::{DateTime, Utc};
    use futures::{SinkExt, StreamExt};
    use quark_im::{framed_stream::FramedStream, io_stream::IoStream, message_pack::MessagePack, negotiator::Negotiator};
    use tokio::io::{AsyncRead, AsyncWrite};

    pub const NAME: &'static str = "SpeedTestService";

    pub async fn start<I>(mut stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        let mut negotiator = Negotiator::new(&mut stream);
        negotiator.send(NAME.to_string()).await?;

        let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<DateTime<Utc>, DateTime<Utc>>::default());

        stream.send(Utc::now()).await?;
        stream.send(Utc::now()).await?;
        stream.send(Utc::now()).await?;

        while let Some(Ok(message)) = stream.next().await {
            tracing::info!(?message);
        }

        Ok(())
    }

    pub async fn handle<I>(stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<DateTime<Utc>, DateTime<Utc>>::default());

        while let Some(Ok(_message)) = stream.next().await {
            stream.send(Utc::now()).await?;
        }

        Ok(())
    }
}

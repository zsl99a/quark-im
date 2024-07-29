use std::{future::Future, net::SocketAddr};

use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use s2n_quic::{client::Connect, stream::BidirectionalStream, Client, Connection, Server};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
};

use crate::{
    framed_stream::FramedStream,
    io_stream::IoStream,
    message_pack::MessagePack,
    negotiator::Negotiator,
    routing::{PeerInfo, Routing},
};

pub async fn session_online<I, Fut>(stream: I, routing: Routing, future: Fut) -> Result<()>
where
    I: AsyncRead + AsyncWrite + Send + Unpin,
    Fut: Future<Output = Result<()>> + Send,
{
    let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<PeerInfo, PeerInfo>::default());

    let peer_id = routing.lock().await.peer_id;
    let peer_info = routing.lock().await.peers.get(&peer_id).context("peer_id not found")?.clone();
    stream.send(peer_info.clone()).await?;

    let peer_info = stream.next().await.context("failed to receive peer_info")??;

    let mut lock = routing.lock().await;
    if lock.peers.contains_key(&peer_info.peer_id) {
        anyhow::bail!("peer_id already exists");
    }
    lock.peers.insert(peer_info.peer_id, peer_info.clone());
    drop(lock);

    future.await?;

    routing.lock().await.peers.remove(&peer_info.peer_id);

    Ok(())
}

pub async fn connection_starting<H, Fut>(mut receiver: mpsc::Receiver<SocketAddr>, client: Client, mut server: Server, start_service: H)
where
    H: Fn(BidirectionalStream, Connection) -> Fut + Send + Clone + 'static,
    Fut: Future<Output = Result<()>> + Send,
{
    let client_loop = async {
        while let Some(connect_endpoint) = receiver.recv().await {
            let client = client.clone();
            let start_service = start_service.clone();

            tokio::spawn(async move {
                let mut connection = client.connect(Connect::new(connect_endpoint).with_server_name("localhost")).await?;
                connection.keep_alive(true)?;
                let stream = connection.open_bidirectional_stream().await?;
                start_service(stream, connection).await
            });
        }
    };

    let server_loop = async {
        while let Some(mut connection) = server.accept().await {
            let start_service = start_service.clone();

            tokio::spawn(async move {
                let stream = connection.accept_bidirectional_stream().await?.context("failed to accept stream")?;
                start_service(stream, connection).await
            });
        }
    };

    tokio::select! {
        _ = client_loop => {},
        _ = server_loop => {},
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ServiceMode {
    Start,
    Handle,
}

pub async fn stream_starting<N, F, Fut>(mut rx: mpsc::Receiver<N>, connection: Connection, handle_stream: F) -> Result<()>
where
    N: Serialize + DeserializeOwned + Send + Clone + 'static,
    F: Fn(BidirectionalStream, N, ServiceMode) -> Fut + Send + Clone + 'static,
    Fut: Future<Output = Result<()>> + Send,
{
    let (handle, mut acceptor) = connection.split();

    let opener_future = async {
        while let Some(service_name) = rx.recv().await {
            let mut handle = handle.clone();
            let handle_stream = handle_stream.clone();

            tokio::spawn(async move {
                let mut stream = handle.open_bidirectional_stream().await?;

                let mut negotiator = Negotiator::new(&mut stream);
                negotiator.send(service_name.clone()).await?;

                handle_stream(stream, service_name, ServiceMode::Start).await?;

                Result::<()>::Ok(())
            });
        }

        Result::<()>::Ok(())
    };

    let accept_future = async {
        while let Ok(Some(mut stream)) = acceptor.accept_bidirectional_stream().await {
            let handle_stream = handle_stream.clone();

            tokio::spawn(async move {
                let mut negotiator = Negotiator::new(&mut stream);
                let service_name = negotiator.recv().await?;

                handle_stream(stream, service_name, ServiceMode::Handle).await?;

                Result::<()>::Ok(())
            });
        }

        Result::<()>::Ok(())
    };

    tokio::select! {
        _ = opener_future => {}
        _ = accept_future => {}
    }

    Ok(())
}

use std::{future::Future, net::SocketAddr};

use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use s2n_quic::{client::Connect, stream::BidirectionalStream, Client, Connection, Server};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
};

use crate::{
    framed_stream::FramedStream,
    io_stream::IoStream,
    message_pack::MessagePack,
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

pub async fn worker_starting<H, Fut>(receiver: mpsc::Receiver<SocketAddr>, client: Client, server: Server, start_service: H)
where
    H: Fn(BidirectionalStream, Connection) -> Fut + Send + Clone + 'static,
    Fut: Future<Output = Result<()>> + Send,
{
    tokio::select! {
        _ = client_loop(receiver, client, start_service.clone()) => {},
        _ = server_loop(server, start_service.clone()) => {},
    }
}

async fn client_loop<H, Fut>(mut receiver: mpsc::Receiver<SocketAddr>, client: Client, start_service: H)
where
    H: Fn(BidirectionalStream, Connection) -> Fut + Send + Clone + 'static,
    Fut: Future<Output = Result<()>> + Send,
{
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
}

async fn server_loop<H, Fut>(mut server: Server, start_service: H)
where
    H: Fn(BidirectionalStream, Connection) -> Fut + Send + Clone + 'static,
    Fut: Future<Output = Result<()>> + Send,
{
    while let Some(mut connection) = server.accept().await {
        let start_service = start_service.clone();

        tokio::spawn(async move {
            let stream = connection.accept_bidirectional_stream().await?.context("failed to accept stream")?;
            start_service(stream, connection).await
        });
    }
}

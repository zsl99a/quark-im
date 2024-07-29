use std::net::SocketAddr;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
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

pub const NAME: &'static str = "ReferralService";

pub async fn start<I>(stream: I, routing: Routing, sender: mpsc::Sender<SocketAddr>) -> Result<()>
where
    I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
{
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

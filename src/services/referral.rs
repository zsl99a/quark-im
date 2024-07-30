use std::net::SocketAddr;

use anyhow::Result;
use async_trait::async_trait;
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
    service::Service,
};

#[derive(Debug, Clone)]
pub struct ReferralService {
    routing: Routing,
    sender: mpsc::Sender<SocketAddr>,
}

impl ReferralService {
    pub fn new(routing: Routing, sender: mpsc::Sender<SocketAddr>) -> Self {
        Self { routing, sender }
    }
}

#[async_trait]
impl Service for ReferralService {
    const NAME: &'static str = "ReferralService";

    async fn start<I>(self, stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<PeerInfo, ()>::default());

        while let Some(Ok(peer_info)) = stream.next().await {
            if !self.routing.lock().await.peers.contains_key(&peer_info.peer_id) {
                for endpoint in peer_info.endpoints {
                    self.sender.send(endpoint).await?;
                }
            }
        }

        Ok(())
    }

    async fn handle<I>(self, stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<(), PeerInfo>::default());

        for peer_info in self.routing.lock().await.peers.values().cloned() {
            stream.send(peer_info).await?;
        }

        while let Some(Ok(_message)) = stream.next().await {}

        Ok(())
    }
}

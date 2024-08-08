use anyhow::Result;
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    abstracts::{PeerInfo, Service},
    framed_stream::FramedStream,
    io_stream::IoStream,
    message_pack::MessagePack,
    QuarkIM,
};

pub struct ReferralService {
    im: QuarkIM,
}

impl ReferralService {
    pub fn new(im: QuarkIM) -> Self {
        Self { im }
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
            if !self.im.peers.contains_key(&peer_info.peer_id) {
                for endpoint in peer_info.endpoints {
                    self.im.connect(endpoint).await?;
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

        for peer_info in self.im.peers.iter() {
            stream.send(peer_info.value().clone()).await?;
        }

        while let Some(Ok(_message)) = stream.next().await {}

        Ok(())
    }
}

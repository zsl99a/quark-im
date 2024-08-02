use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

use crate::{framed_stream::FramedStream, io_stream::IoStream, message_pack::MessagePack, abstracts::Service};

pub struct SpeedTestService {
    peer_id: Uuid,
    speeds: Arc<DashMap<Uuid, u64>>,
}

impl SpeedTestService {
    pub fn new(peer_id: Uuid, speeds: Arc<DashMap<Uuid, u64>>) -> Self {
        Self { peer_id, speeds }
    }
}

impl Drop for SpeedTestService {
    fn drop(&mut self) {
        self.speeds.remove(&self.peer_id);
    }
}

#[async_trait]
impl Service for SpeedTestService {
    const NAME: &'static str = "SpeedTestService";

    async fn start<I>(self, stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<(), ()>::default());

        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            let ins = Instant::now();

            stream.send(()).await?;

            stream.next().await.context("failed to receive message")??;

            let elapsed = ins.elapsed().as_micros() as u64;

            self.speeds.entry(self.peer_id).and_modify(|e| *e = (*e * 9 + elapsed) / 10).or_insert(elapsed);

            interval.tick().await;
        }
    }

    async fn handle<I>(self, stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<(), ()>::default());

        while let Some(Ok(message)) = stream.next().await {
            stream.send(message).await?;
        }

        Ok(())
    }
}

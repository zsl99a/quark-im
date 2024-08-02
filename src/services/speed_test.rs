use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

use crate::{abstracts::Service, framed_stream::FramedStream, io_stream::IoStream, message_pack::MessagePack, quark::QuarkIM};

pub struct SpeedTestService {
    im: QuarkIM,
    peer_id: Uuid,
    report: Arc<DashMap<Uuid, BTreeMap<Uuid, u64>>>,
}

impl SpeedTestService {
    pub fn new(im: QuarkIM, peer_id: Uuid, report: Arc<DashMap<Uuid, BTreeMap<Uuid, u64>>>) -> Self {
        Self { im, peer_id, report }
    }
}

impl Drop for SpeedTestService {
    fn drop(&mut self) {
        if let Some(mut report) = self.report.get_mut(&self.im.peer_id) {
            report.value_mut().remove(&self.peer_id);
        }
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

            let mut lock = self.report.entry(self.im.peer_id).or_insert(Default::default());
            if let Some(e) = lock.get_mut(&self.peer_id) {
                *e = (*e * 9 + elapsed) / 10
            } else {
                lock.insert(self.peer_id, elapsed);
            }
            drop(lock);

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

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

use crate::{framed_stream::FramedStream, io_stream::IoStream, message_pack::MessagePack, service::Service};

#[derive(Debug, Clone)]
pub struct SpeedReportService {
    peer_id: Uuid,
    speeds: Arc<DashMap<Uuid, u64>>,
    report: Arc<DashMap<Uuid, BTreeMap<Uuid, u64>>>,
}

impl SpeedReportService {
    pub fn new(peer_id: Uuid, speeds: Arc<DashMap<Uuid, u64>>, report: Arc<DashMap<Uuid, BTreeMap<Uuid, u64>>>) -> Self {
        Self { peer_id, speeds, report }
    }
}

impl Drop for SpeedReportService {
    fn drop(&mut self) {
        self.report.remove(&self.peer_id);
    }
}

#[async_trait]
impl Service for SpeedReportService {
    const NAME: &'static str = "SpeedTestReportService";

    async fn start<I>(self, stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<(), BTreeMap<Uuid, u64>>::default());

        tokio::time::sleep(Duration::from_secs(3)).await;

        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            let mut speeds = BTreeMap::new();
            for item in self.speeds.iter() {
                speeds.insert(*item.key(), *item.value());
            }
            stream.send(speeds).await?;
            interval.tick().await;
        }
    }

    async fn handle<I>(self, stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<BTreeMap<Uuid, u64>, ()>::default());

        while let Some(Ok(speeds)) = stream.next().await {
            self.report.insert(self.peer_id, speeds);
        }

        Ok(())
    }
}

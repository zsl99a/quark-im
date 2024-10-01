use std::{collections::BTreeMap, sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

use crate::{abstracts::Service, framed_stream::FramedStream, io_stream::IoStream, message_pack::MessagePack, QuarkIM};

pub struct SpeedReportService {
    im: QuarkIM,
    peer_id: Uuid,
    report: Arc<DashMap<Uuid, BTreeMap<Uuid, u64>>>,
}

impl SpeedReportService {
    pub fn new(im: QuarkIM, peer_id: Uuid, report: Arc<DashMap<Uuid, BTreeMap<Uuid, u64>>>) -> Self {
        Self { im, peer_id, report }
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

    async fn client<I>(self, stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        let mut stream = FramedStream::new(IoStream::new(stream), MessagePack::<(), BTreeMap<Uuid, u64>>::default());

        tokio::time::sleep(Duration::from_secs(3)).await;

        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            if let Some(speeds) = self.report.get(&self.im.peer_id) {
                stream.send(speeds.value().clone()).await?;
            }
            interval.tick().await;
        }
    }

    async fn server<I>(self, stream: I) -> Result<()>
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

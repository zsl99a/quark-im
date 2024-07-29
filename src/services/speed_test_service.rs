use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{framed_stream::FramedStream, io_stream::IoStream, message_pack::MessagePack};

pub const NAME: &'static str = "SpeedTestService";

pub async fn start<I>(stream: I) -> Result<()>
where
    I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
{
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

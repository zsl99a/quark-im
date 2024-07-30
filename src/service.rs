use anyhow::Result;
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

#[async_trait]
pub trait Service {
    const NAME: &'static str;

    async fn start<I>(self, stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin;

    async fn handle<I>(self, stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin;
}

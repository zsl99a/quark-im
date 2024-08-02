use anyhow::Result;
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::abstracts::Service;

pub struct RelayService {}

#[async_trait]
impl Service for RelayService {
    const NAME: &'static str = "RelayService";

    async fn start<I>(self, _stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        Ok(())
    }

    async fn handle<I>(self, _stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin,
    {
        Ok(())
    }
}

use std::fmt::Debug;

use anyhow::Result;
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

#[async_trait]
pub trait Service<I>
where
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    fn service_id(&self) -> impl Debug;

    async fn start(&self, stream: I) -> Result<()>;

    async fn handle(&self, stream: I) -> Result<()>;
}

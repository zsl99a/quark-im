use std::{fmt::Debug, net::SocketAddr};

use anyhow::Result;
use async_trait::async_trait;
use s2n_quic::stream::BidirectionalStream;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

use crate::QuarkIM;

#[async_trait]
pub trait Service {
    const NAME: &'static str;

    async fn client<I>(self, stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin;

    async fn server<I>(self, stream: I) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Sync + Unpin;
}

#[allow(unused_variables)]
#[async_trait]
pub trait QuarkIMHook: Debug + Send + Sync + 'static {
    fn client_side(&self, im: &QuarkIM, peer_id: Uuid) {}

    fn server_side(&self, im: &QuarkIM, peer_id: Uuid) {}

    fn both_sides(&self, im: &QuarkIM, peer_id: Uuid) {}

    async fn handle(&self, im: &QuarkIM, stream: BidirectionalStream, peer_id: Uuid, name: String, mode: ServiceMode) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ServiceMode {
    Client,
    Server,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: Uuid,
    pub endpoints: Vec<SocketAddr>,
}

impl Default for PeerInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl PeerInfo {
    pub fn new() -> Self {
        Self {
            peer_id: Uuid::new_v4(),
            endpoints: vec![],
        }
    }
}

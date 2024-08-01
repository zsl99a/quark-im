use std::{net::SocketAddr, sync::Arc};

use dashmap::DashMap;
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::routing::PeerInfo;

#[derive(Debug, Clone)]
pub struct QuarkIM<N, E>
where
    N: Serialize + DeserializeOwned,
{
    pub peers: Arc<DashMap<Uuid, Peer<N, E>>>,
    pub new_conn_tx: mpsc::Sender<SocketAddr>,
}

impl<N, E> QuarkIM<N, E>
where
    N: Serialize + DeserializeOwned,
{
    pub fn new() -> Self {
        todo!()
    }
}

pub struct QuarkIMTask {}

#[derive(Debug, Clone)]
pub struct Peer<N, E>
where
    N: Serialize + DeserializeOwned,
{
    pub info: PeerInfo,
    pub extra_info: E,
    pub new_stream_tx: mpsc::Sender<N>,
}

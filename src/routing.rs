use std::{collections::BTreeMap, net::SocketAddr, ops::Deref, sync::Arc};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Routing(Arc<Mutex<RoutingStore>>);

impl Deref for Routing {
    type Target = Arc<Mutex<RoutingStore>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Routing {
    pub fn new(local_info: PeerInfo) -> Self {
        let store = RoutingStore {
            leader: true,
            peer_id: local_info.peer_id,
            peers: BTreeMap::from([(local_info.peer_id, local_info)]),
        };
        Self(Arc::new(Mutex::new(store)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingStore {
    pub leader: bool,
    pub peer_id: Uuid,
    pub peers: BTreeMap<Uuid, PeerInfo>,
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

use std::{collections::VecDeque, fmt::Debug, net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use dashmap::DashMap;
use s2n_quic::{client::Connect, connection::Handle, stream::BidirectionalStream, Client, Connection, Server};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::{
    abstracts::{PeerInfo, QuarkIMHook, ServiceMode},
    negotiator::Negotiator,
};

#[derive(Debug, Clone)]
pub struct QuarkIM {
    pub client: Client,
    pub peer_id: Uuid,
    pub peers: Arc<DashMap<Uuid, PeerInfo>>,
    pub handles: Arc<DashMap<Uuid, Handle>>,
    pub hooks: Arc<dyn QuarkIMHook>,
}

impl QuarkIM {
    pub async fn new<H>(hooks: H, client: Client, server: Server) -> Result<Self>
    where
        H: QuarkIMHook,
    {
        let mut local_info = PeerInfo::new();
        local_info.endpoints.push(SocketAddr::from(([127, 0, 0, 1], server.local_addr()?.port())));

        let peers = Arc::new(DashMap::new());
        peers.insert(local_info.peer_id, local_info.clone());

        let im = QuarkIM {
            client,
            peer_id: local_info.peer_id,
            peers,
            handles: Arc::new(DashMap::new()),
            hooks: Arc::new(hooks),
        };

        im.server_task(server);

        if let Ok(endpoint) = dotenvy::var("QUARK_CONNECT_ENDPOINT").unwrap_or_default().parse::<SocketAddr>() {
            im.connect(endpoint).await?;
        }

        Ok(im)
    }

    pub fn start_service_task(&self, peer_id: Uuid, name: String) {
        self.start_service_task_with(peer_id, name, VecDeque::new())
    }

    pub async fn open_service_stream(&self, peer_id: Uuid, name: String) -> Result<BidirectionalStream> {
        self.open_service_stream_with(peer_id, name, VecDeque::new()).await
    }

    pub async fn open_stream(&self, peer_id: Uuid) -> Result<BidirectionalStream> {
        self.open_stream_with(peer_id, VecDeque::new()).await
    }

    pub fn start_service_task_with(&self, peer_id: Uuid, name: String, link_info: VecDeque<Uuid>) {
        let im = self.clone();
        tokio::spawn(async move {
            let stream = im.clone().open_service_stream_with(peer_id, name.clone(), link_info).await?;
            im.hooks.handle(&im, stream, peer_id, name, ServiceMode::Start).await
        });
    }

    pub async fn open_service_stream_with(&self, peer_id: Uuid, name: String, link_info: VecDeque<Uuid>) -> Result<BidirectionalStream> {
        let mut stream = self.open_stream_with(peer_id, link_info).await?;

        let mut negotiator = Negotiator::new(&mut stream);
        negotiator.send(name).await?;

        Ok(stream)
    }

    pub async fn open_stream_with(&self, peer_id: Uuid, link_info: VecDeque<Uuid>) -> Result<BidirectionalStream> {
        if let Some(handle) = self.handles.get(&peer_id) {
            let mut handle = handle.value().clone();

            let mut stream = handle.open_bidirectional_stream().await?;

            let mut negotiator = Negotiator::<VecDeque<Uuid>, _>::new(&mut stream);
            negotiator.send(link_info).await?;

            Ok(stream)
        } else {
            anyhow::bail!("peer not found");
        }
    }

    pub async fn connect(&self, endpoint: SocketAddr) -> Result<()> {
        let mut connection = self.client.connect(Connect::new(endpoint).with_server_name("localhost")).await?;

        let im = self.clone();
        let stream = connection.open_bidirectional_stream().await?;

        tokio::spawn(async move { im.connection_handle(stream, connection, ServiceMode::Start).await });

        Ok(())
    }
}

impl QuarkIM {
    async fn connection_handle(&self, mut stream: BidirectionalStream, connection: Connection, mode: ServiceMode) -> Result<()> {
        let (handle, mut acceptor) = connection.split();

        let mut negotiator = Negotiator::new(&mut stream);

        let peer_info = self.peers.get(&self.peer_id).context("failed to get peer info")?.clone();

        negotiator.send(peer_info).await?;

        let peer_info = negotiator.recv().await?;

        let mut is_new_peer = false;
        self.handles.entry(peer_info.peer_id).or_insert_with(|| {
            is_new_peer = true;
            self.peers.insert(peer_info.peer_id, peer_info.clone());
            handle
        });

        if is_new_peer {
            let peer_id = peer_info.peer_id;

            match mode {
                ServiceMode::Start => self.hooks.client_side(&self, peer_id),
                ServiceMode::Handle => self.hooks.server_side(&self, peer_id),
            }

            self.hooks.both_sides(&self, peer_id);

            while let Some(mut stream) = acceptor.accept_bidirectional_stream().await? {
                let im = self.clone();

                tokio::spawn(async move {
                    let mut negotiator = Negotiator::<VecDeque<Uuid>, _>::new(&mut stream);
                    let mut link_info = negotiator.recv().await?;

                    if let Some(next_peer_id) = link_info.pop_front() {
                        let mut relay_stream = im.open_stream_with(next_peer_id, link_info).await?;

                        while let Err(error) = tokio::io::copy_bidirectional(&mut stream, &mut relay_stream).await {
                            tracing::error!(?error, "relay stream error")
                        }
                    } else {
                        let mut negotiator = Negotiator::new(&mut stream);
                        let service_name = negotiator.recv().await?;

                        im.hooks.handle(&im, stream, peer_id, service_name, ServiceMode::Handle).await?;
                    }

                    Result::<()>::Ok(())
                });
            }

            self.peers.remove(&peer_info.peer_id);
            self.handles.remove(&peer_info.peer_id);
        }

        Ok(())
    }

    fn server_task(&self, mut server: Server) -> JoinHandle<()> {
        let im = self.clone();

        tokio::spawn(async move {
            while let Some(mut connection) = server.accept().await {
                let im = im.clone();

                tokio::spawn(async move {
                    let stream = connection.accept_bidirectional_stream().await?.context("failed to accept stream")?;

                    im.connection_handle(stream, connection, ServiceMode::Handle).await
                });
            }
        })
    }
}

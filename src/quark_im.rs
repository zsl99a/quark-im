use std::{collections::VecDeque, fmt::Debug, net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use dashmap::{mapref::one::Ref, DashMap};
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
    pub fn new<H>(client: Client, local_info: PeerInfo, hooks: H) -> Self
    where
        H: QuarkIMHook,
    {
        QuarkIM {
            client,
            peer_id: local_info.peer_id,
            peers: Arc::new(DashMap::from_iter([(local_info.peer_id, local_info)])),
            handles: Arc::new(DashMap::new()),
            hooks: Arc::new(hooks),
        }
    }

    pub fn local_info(&self) -> Result<Ref<Uuid, PeerInfo>> {
        self.peers.get(&self.peer_id).context("failed to get local info")
    }

    pub fn start_service_task(&self, peer_id: Uuid, name: String) {
        self.start_service_task_with_link(name, VecDeque::from([peer_id]))
    }

    pub async fn open_service_stream(&self, peer_id: Uuid, name: String) -> Result<BidirectionalStream> {
        self.open_service_stream_with_link(name, VecDeque::from([peer_id])).await
    }

    pub async fn open_stream(&self, peer_id: Uuid) -> Result<BidirectionalStream> {
        self.open_stream_with_link(VecDeque::from([peer_id])).await
    }

    pub fn start_service_task_with_link(&self, name: String, link_info: VecDeque<Uuid>) {
        let im = self.clone();
        tokio::spawn(async move {
            let peer_id = *link_info.get(link_info.len() - 1).context("failed to get peer id")?;
            let stream = im.clone().open_service_stream_with_link(name.clone(), link_info).await?;
            im.hooks.handle(&im, stream, peer_id, name, ServiceMode::Start).await
        });
    }

    pub async fn open_service_stream_with_link(&self, name: String, link_info: VecDeque<Uuid>) -> Result<BidirectionalStream> {
        let mut stream = self.open_stream_with_link(link_info).await?;

        let mut negotiator = Negotiator::new(&mut stream);
        negotiator.send(name).await?;

        Ok(stream)
    }

    pub async fn open_stream_with_link(&self, mut link_info: VecDeque<Uuid>) -> Result<BidirectionalStream> {
        let peer_id = link_info.pop_front().context("failed to get peer id")?;

        let mut handle = self.handles.get(&peer_id).context("failed to get handle")?.clone();

        let mut stream = handle.open_bidirectional_stream().await?;

        let mut negotiator = Negotiator::<VecDeque<Uuid>, _>::new(&mut stream);
        negotiator.send(link_info).await?;

        Ok(stream)
    }

    pub async fn connect(&self, endpoint: SocketAddr) -> Result<()> {
        let mut connection = self.client.connect(Connect::new(endpoint).with_server_name("localhost")).await?;

        let im = self.clone();
        let stream = connection.open_bidirectional_stream().await?;

        tokio::spawn(async move { im.connection_handle(stream, connection, ServiceMode::Start).await });

        Ok(())
    }

    pub fn server_task(&self, mut server: Server) -> JoinHandle<()> {
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

impl QuarkIM {
    async fn connection_handle(&self, mut stream: BidirectionalStream, connection: Connection, mode: ServiceMode) -> Result<()> {
        let (handle, mut acceptor) = connection.split();

        let mut negotiator = Negotiator::new(&mut stream);

        let local_info = self.local_info()?.clone();

        negotiator.send(local_info).await?;

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
                    let link_info = negotiator.recv().await?;

                    if !link_info.is_empty() {
                        let mut relay_stream = im.open_stream_with_link(link_info).await?;

                        while let Err(error) = tokio::io::copy_bidirectional(&mut stream, &mut relay_stream).await {
                            tracing::error!(?error, "relay stream error");
                            break;
                        }
                    } else {
                        let mut negotiator = Negotiator::new(&mut stream);
                        let service_name: String = negotiator.recv().await?;

                        if let Err(error) = im.hooks.handle(&im, stream, peer_id, service_name.clone(), ServiceMode::Handle).await {
                            tracing::warn!(?error, ?peer_id, service_name, "failed to handle service");
                        }
                    }

                    Result::<()>::Ok(())
                });
            }

            self.peers.remove(&peer_info.peer_id);
            self.handles.remove(&peer_info.peer_id);
        }

        Ok(())
    }
}

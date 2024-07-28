use std::{future::Future, net::SocketAddr};

use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use s2n_quic::{client::Connect, stream::BidirectionalStream, Client, Server};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
};

use crate::{
    framed_stream::FramedStream,
    io_stream::IoStream,
    message_pack::MessagePack,
    negotiator::Negotiator,
    routing::{PeerInfo, Routing},
};

pub async fn session_online<I, Fut>(stream: IoStream<I>, routing: Routing, future: Fut) -> Result<()>
where
    I: AsyncRead + AsyncWrite + Send + Unpin,
    Fut: Future<Output = ()> + Send,
{
    let mut stream = FramedStream::new(stream, MessagePack::<PeerInfo, PeerInfo>::default());

    let peer_id = routing.lock().await.peer_id;
    let peer_info = routing.lock().await.peers.get(&peer_id).context("peer_id not found")?.clone();
    stream.send(peer_info.clone()).await?;

    let peer_info = stream.next().await.context("failed to receive peer_info")??;
    routing.lock().await.peers.insert(peer_info.peer_id, peer_info.clone());

    future.await;

    routing.lock().await.peers.remove(&peer_info.peer_id);

    Ok(())
}

pub async fn client_starting<H, Fut, Svc>(mut receiver: mpsc::Receiver<SocketAddr>, client: Client, routing: Routing, start_service: H)
where
    H: Fn(BidirectionalStream, Routing, Svc) -> Fut + Send + Copy + 'static,
    Fut: Future<Output = Result<()>> + Send,
    Svc: Serialize + DeserializeOwned + Send + Clone + 'static,
{
    while let Some(connect_endpoint) = receiver.recv().await {
        let client = client.clone();
        let routing = routing.clone();

        tokio::spawn(async move {
            let mut connection = client.connect(Connect::new(connect_endpoint).with_server_name("localhost")).await?;
            connection.keep_alive(true)?;

            let stream = IoStream::new(connection.open_bidirectional_stream().await?);

            session_online(stream, routing.clone(), async move {
                let (_handle, mut acceptor) = connection.split();

                // service
                while let Ok(Some(mut stream)) = acceptor.accept_bidirectional_stream().await {
                    let routing = routing.clone();
                    tokio::spawn(async move {
                        // negotiator
                        let mut negotiator = Negotiator::<Svc, _>::new(&mut stream);
                        let service = negotiator.recv().await?;
                        negotiator.send(service.clone()).await?;
                        drop(negotiator);

                        // service
                        start_service(stream, routing, service).await?;

                        Result::<()>::Ok(())
                    });
                }
            })
            .await
        });
    }
}

pub async fn server_starting<H, Fut, Svc>(mut server: Server, routing: Routing, start_service: H)
where
    H: Fn(BidirectionalStream, Routing, Svc) -> Fut + Send + Copy + 'static,
    Fut: Future<Output = Result<()>> + Send,
    Svc: Serialize + DeserializeOwned + Send + Clone + 'static,
{
    while let Some(mut connection) = server.accept().await {
        let routing = routing.clone();

        tokio::spawn(async move {
            let stream = IoStream::new(connection.accept_bidirectional_stream().await?.context("failed to accept stream")?);

            session_online(stream, routing.clone(), async move {
                let (_handle, mut acceptor) = connection.split();

                // service
                while let Ok(Some(mut stream)) = acceptor.accept_bidirectional_stream().await {
                    let routing = routing.clone();
                    tokio::spawn(async move {
                        // negotiator
                        let mut negotiator = Negotiator::<Svc, _>::new(&mut stream);
                        let service = negotiator.recv().await?;
                        negotiator.send(service.clone()).await?;
                        drop(negotiator);

                        // service
                        start_service(stream, routing, service).await?;

                        Result::<()>::Ok(())
                    });
                }
            })
            .await
        });
    }
}

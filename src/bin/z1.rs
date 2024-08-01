use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use quark_im::quic::{create_client, create_server};
use s2n_quic::client::Connect;
use tokio_serde::{formats::MessagePack, Framed};
use tokio_util::codec::{Decoder, LengthDelimitedCodec};
use tracing::Level;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_file(true).with_line_number(true).with_max_level(Level::INFO).init();

    tokio::spawn(server());

    tokio::spawn(client());

    tokio::time::sleep(Duration::from_secs(100)).await;

    Ok(())
}

async fn server() -> Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], 12345));

    let mut server = create_server(addr).await?;

    while let Some(mut connection) = server.accept().await {
        tokio::spawn(async move {
            while let Ok(Some(stream)) = connection.accept_bidirectional_stream().await {
                tokio::spawn(async move {
                    let mut stream = Framed::<_, String, String, _>::new(LengthDelimitedCodec::new().framed(stream), MessagePack::<String, String>::default());

                    while let Some(Ok(message)) = stream.next().await {
                        stream.send(message).await?;
                    }

                    Result::<()>::Ok(())
                });
            }
        });
    }

    Ok(())
}

async fn client() -> Result<()> {
    let client = create_client(SocketAddr::from(([0, 0, 0, 0], 0))).await?;

    let mut connection = client
        .connect(Connect::new(SocketAddr::from(([127, 0, 0, 1], 12345))).with_server_name("localhost"))
        .await?;
    connection.keep_alive(true)?;

    let stream = connection.open_bidirectional_stream().await?;

    let mut stream = Framed::<_, String, String, _>::new(LengthDelimitedCodec::new().framed(stream), MessagePack::<String, String>::default());

    stream.send("Hello World".into()).await?;
    let _ = stream.next().await;

    let ins = Instant::now();

    let msg_count = 1000;

    for i in 0..msg_count {
        stream.send(format!("Hello World {}", i)).await?;
    }

    let mut count: i64 = 0;

    while let Some(Ok(message)) = stream.next().await {
        count += 1;
        if count == msg_count {
            tracing::info!(elapsed = ?ins.elapsed(), message)
        }
    }

    Ok(())
}

use std::{net::SocketAddr, path::Path};

use anyhow::Result;
use s2n_quic::{provider::tls, Client, Server};

pub static CA_CERT_PEM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/certs/ca-cert.pem");

pub static SERVER_CERT_PEM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/certs/server-cert.pem");
pub static SERVER_KEY_PEM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/certs/server-key.pem");

pub static CLIENT_CERT_PEM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/certs/client-cert.pem");
pub static CLIENT_KEY_PEM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/certs/client-key.pem");

pub async fn create_client(addr: SocketAddr) -> Result<Client> {
    let tls = tls::default::Client::builder()
        .with_certificate(Path::new(CA_CERT_PEM))?
        .with_client_identity(Path::new(CLIENT_CERT_PEM), Path::new(CLIENT_KEY_PEM))?
        .build()?;

    Ok(Client::builder().with_tls(tls)?.with_io(addr)?.start()?)
}

pub async fn create_server(addr: SocketAddr) -> Result<Server> {
    let tls = tls::default::Server::builder()
        .with_trusted_certificate(Path::new(CA_CERT_PEM))?
        .with_certificate(Path::new(SERVER_CERT_PEM), Path::new(SERVER_KEY_PEM))?
        .with_client_authentication()?
        .build()?;

    Ok(Server::builder().with_tls(tls)?.with_io(addr)?.start()?)
}

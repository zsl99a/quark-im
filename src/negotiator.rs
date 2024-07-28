use std::marker::PhantomData;

use anyhow::{Error, Result};
use bytes::{Bytes, BytesMut};
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug)]
pub struct Negotiator<'a, T, I>
where
    T: Serialize + DeserializeOwned + Send,
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    io: &'a mut I,
    _marker: PhantomData<T>,
}

impl<'a, T, I> Negotiator<'a, T, I>
where
    T: Serialize + DeserializeOwned + Send,
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    pub fn new(io: &'a mut I) -> Self {
        Self { io, _marker: PhantomData }
    }
}

impl<'a, T, I> Negotiator<'a, T, I>
where
    T: Serialize + DeserializeOwned + Send,
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    pub async fn recv(&mut self) -> Result<T>
    where
        I: AsyncRead + AsyncWrite + Send + Unpin,
    {
        let mut src = BytesMut::new();
        self.io.read_buf(&mut src).await?;
        Ok(rmp_serde::from_slice(&src)?)
    }

    pub async fn send(&mut self, msg: T) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Unpin,
    {
        let dst = Bytes::from(rmp_serde::to_vec(&msg)?);
        self.io.write_all(&dst).await.map_err(Error::new)
    }
}

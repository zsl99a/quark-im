use std::{io::ErrorKind, marker::PhantomData};

use anyhow::{Error, Result};
use bytes::{BufMut, Bytes, BytesMut};
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
        let len = self.io.read_u16().await?;
        let mut src = BytesMut::new();
        while let Ok(item) = self.io.read_u8().await {
            src.put_u8(item);
            if src.len() == (len as usize) {
                return Ok(rmp_serde::from_slice(&src)?);
            }
        }
        Err(Error::new(std::io::Error::new(ErrorKind::Other, "failed to read negotiator message")))
    }

    pub async fn send(&mut self, msg: T) -> Result<()>
    where
        I: AsyncRead + AsyncWrite + Send + Unpin,
    {
        let dst = Bytes::from(rmp_serde::to_vec(&msg)?);
        self.io.write_u16(dst.len() as u16).await?;
        self.io.write_all(&dst).await.map_err(Error::new)
    }
}

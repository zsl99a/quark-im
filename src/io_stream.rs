use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::{Bytes, BytesMut};
use futures::{FutureExt, Sink, Stream};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};

pub struct IoStream<I>
where
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    io: I,
    buffer: VecDeque<BytesMut>,
    read_buf: BytesMut,
    write_buf: BytesMut,
    codec: LengthDelimitedCodec,
}

impl<I> IoStream<I>
where
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    pub fn new(io: I) -> Self {
        Self {
            io,
            buffer: VecDeque::new(),
            read_buf: BytesMut::new(),
            write_buf: BytesMut::new(),
            codec: LengthDelimitedCodec::new(),
        }
    }
}

impl<I> Stream for IoStream<I>
where
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    type Item = std::io::Result<BytesMut>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(msg) = self.buffer.pop_front() {
            return Poll::Ready(Some(Ok(msg)));
        }

        let mut src = self.read_buf.split();

        loop {
            match self.io.read_buf(&mut src).boxed().poll_unpin(cx) {
                Poll::Ready(Ok(_n)) => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(e))),
                Poll::Pending => break,
            }
        }

        if src.is_empty() {
            return Poll::Pending;
        }

        while let Ok(Some(message)) = self.codec.decode(&mut src) {
            self.buffer.push_back(message);
        }

        self.read_buf = src;

        if self.buffer.is_empty() {
            return Poll::Pending;
        }

        Poll::Ready(self.buffer.pop_front().map(Ok))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.buffer.len(), None)
    }
}

impl<I> Sink<Bytes> for IoStream<I>
where
    I: AsyncRead + AsyncWrite + Send + Unpin,
{
    type Error = std::io::Error;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Bytes) -> std::io::Result<()> {
        let mut dst = BytesMut::new();
        self.codec.encode(item, &mut dst)?;
        self.write_buf.extend_from_slice(&dst);
        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let mut dst = self.write_buf.split();
        let _ = self.io.write_buf(&mut dst).boxed().poll_unpin(cx)?;
        Poll::Ready(Ok(()))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.io.shutdown().boxed().poll_unpin(cx)
    }
}

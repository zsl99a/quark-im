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
            read_buf: BytesMut::with_capacity(4096),
            write_buf: BytesMut::with_capacity(4096),
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

        let read_buf = unsafe { &mut *(&mut self.read_buf as *mut _) };

        let mut is_eof = false;

        loop {
            match self.io.read_buf(read_buf).boxed().poll_unpin(cx) {
                Poll::Ready(Ok(n)) => {
                    if n == 0 {
                        is_eof = true;
                        break;
                    }
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(e))),
                Poll::Pending => break,
            }
        }

        while let Ok(Some(message)) = self.codec.decode(read_buf) {
            self.buffer.push_back(message);
        }

        if self.buffer.is_empty() {
            if is_eof {
                return Poll::Ready(None);
            }

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
        let write_buf = unsafe { &mut *(&mut self.write_buf as *mut _) };
        self.codec.encode(item, write_buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let write_buf = unsafe { &mut *(&mut self.write_buf as *mut _) };
        self.io.write_buf(write_buf).boxed().poll_unpin(cx).map_ok(|_n| ())
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.io.shutdown().boxed().poll_unpin(cx)
    }
}

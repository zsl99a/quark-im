use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use anyhow::{Error, Result};
use bytes::{Bytes, BytesMut};
use futures::{Sink, SinkExt, Stream, StreamExt};
use serde::{de::DeserializeOwned, Serialize};

use crate::serde_framed::SerdeFramed;

pub struct FramedStream<S, O, I, F>
where
    S: Stream<Item = std::io::Result<BytesMut>> + Sink<Bytes> + Unpin,
    F: SerdeFramed<O, I> + Unpin,
    O: DeserializeOwned + Unpin,
    I: Serialize + Unpin,
{
    stream: S,
    framed: F,
    _marker: PhantomData<(O, I)>,
}

impl<S, O, I, F> FramedStream<S, O, I, F>
where
    S: Stream<Item = std::io::Result<BytesMut>> + Sink<Bytes> + Unpin,
    F: SerdeFramed<O, I> + Unpin,
    O: DeserializeOwned + Unpin,
    I: Serialize + Unpin,
{
    pub fn new(stream: S, framed: F) -> Self {
        Self {
            stream,
            framed,
            _marker: PhantomData,
        }
    }
}

impl<S, O, I, F> Stream for FramedStream<S, O, I, F>
where
    S: Stream<Item = std::io::Result<BytesMut>> + Sink<Bytes> + Unpin,
    F: SerdeFramed<O, I> + Unpin,
    O: DeserializeOwned + Unpin,
    I: Serialize + Unpin,
{
    type Item = Result<O>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.stream.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(message))) => {
                let message = self.framed.deserialize(&message)?;
                Poll::Ready(Some(Ok(message)))
            }
            Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(Error::new(err)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S, O, I, F> Sink<I> for FramedStream<S, O, I, F>
where
    S: Stream<Item = std::io::Result<BytesMut>> + Sink<Bytes> + Unpin,
    F: SerdeFramed<O, I> + Unpin,
    O: DeserializeOwned + Unpin,
    I: Serialize + Unpin,
    S::Error: Send + Sync + std::error::Error + 'static,
{
    type Error = Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        self.stream.poll_ready_unpin(cx).map_err(Error::new)
    }

    fn start_send(mut self: Pin<&mut Self>, item: I) -> Result<()> {
        let dst = self.framed.serialize(item)?;
        self.stream.start_send_unpin(dst).map_err(Error::new)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        self.stream.poll_flush_unpin(cx).map_err(Error::new)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        self.stream.poll_close_unpin(cx).map_err(Error::new)
    }
}

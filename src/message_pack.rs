use std::marker::PhantomData;

use anyhow::Result;
use bytes::{Bytes, BytesMut};
use serde::{de::DeserializeOwned, Serialize};

use crate::serde_framed::SerdeFramed;

#[derive(Debug)]
pub struct MessagePack<O, I>
where
    O: DeserializeOwned,
    I: Serialize,
{
    _marker: PhantomData<(O, I)>,
}

impl<O, I> Default for MessagePack<O, I>
where
    O: DeserializeOwned,
    I: Serialize,
{
    fn default() -> Self {
        Self { _marker: PhantomData }
    }
}

impl<O, I> SerdeFramed<O, I> for MessagePack<O, I>
where
    O: DeserializeOwned,
    I: Serialize,
{
    fn deserialize(&self, item: &BytesMut) -> Result<O> {
        Ok(rmp_serde::from_slice(item)?)
    }

    fn serialize(&self, item: I) -> Result<Bytes> {
        Ok(Bytes::from(rmp_serde::to_vec(&item)?))
    }
}

use anyhow::Result;
use bytes::{Bytes, BytesMut};
use serde::{de::DeserializeOwned, Serialize};

pub trait SerdeFramed<O, I>
where
    O: DeserializeOwned,
    I: Serialize,
{
    fn deserialize(&self, item: &BytesMut) -> Result<O>;

    fn serialize(&self, item: I) -> Result<Bytes>;
}

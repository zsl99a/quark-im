use std::fmt::Debug;

use async_trait::async_trait;

#[async_trait]
pub trait Service<I> {
    fn service_id(&self) -> impl Debug;
}

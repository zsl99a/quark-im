use futures::Future;
use tokio::task::JoinSet;

pub fn join_task<Fut>(future: Fut) -> JoinSet<Fut::Output>
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    let mut join_set = JoinSet::new();
    join_set.spawn(future);
    join_set
}

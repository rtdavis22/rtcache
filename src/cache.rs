use std::hash::Hash;
use std::sync::Arc;

use async_trait::async_trait;
use moka::future::FutureExt;
use moka::notification::ListenerFuture;

pub struct Cache<K, V> {
    cache: moka::future::Cache<K, V>,
    fetcher: Box<dyn Fetcher<V>>,
}

impl<K, V> Cache<K, V>
where
    K: Hash + Eq + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    pub fn new(
        fetcher: impl Fetcher<V> + 'static,
        updater: impl Updater<K, V> + Send + Sync + 'static,
    ) -> Self {
        let updater = Arc::new(updater);
        let eviction_listener = move |k: Arc<K>, v: V, _cause| -> ListenerFuture {
            let updater = updater.clone();
            async move {
                updater.update(k, v).await;
            }
            .boxed()
        };

        Self {
            cache: moka::future::CacheBuilder::new(10_000)
                .async_eviction_listener(eviction_listener)
                .build(),
            fetcher: Box::new(fetcher),
        }
    }

    pub async fn get<Q>(&self, key: K) -> V {
        self.cache
            .get_with(key, async move { self.fetcher.fetch().await })
            .await
    }
}

#[async_trait]
pub trait Fetcher<T> {
    async fn fetch(&self) -> T;
}

#[async_trait]
pub trait Updater<K, V> {
    async fn update(&self, key: Arc<K>, value: V);
}

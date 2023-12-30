use std::hash::Hash;
use std::sync::Arc;

use async_trait::async_trait;
use moka::future::FutureExt;
use moka::notification::ListenerFuture;

pub struct Cache<K, V> {
    cache: moka::future::Cache<K, V>,
    store: Arc<dyn Store<K, V>>,
}

impl<K, V> Cache<K, V>
where
    K: Hash + Eq + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    pub fn new(store: impl Store<K, V> + Send + Sync + 'static) -> Self {
        let store = Arc::new(store);

        let store_clone = store.clone();
        let eviction_listener = move |k: Arc<K>, v: V, _cause| -> ListenerFuture {
            let store = store_clone.clone();
            async move {
                store.update(k, v).await;
            }
            .boxed()
        };

        Self {
            cache: moka::future::CacheBuilder::new(10_000)
                .async_eviction_listener(eviction_listener)
                .build(),
            store,
        }
    }

    pub async fn get(&self, key: &K) -> V
    where
        K: ToOwned<Owned = K>,
    {
        self.cache
            .get_with_by_ref(key, async move { self.store.fetch(key).await })
            .await
    }
}

#[async_trait]
pub trait Store<K, V> {
    async fn fetch(&self, key: &K) -> V;
    async fn update(&self, key: Arc<K>, value: V);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::RwLock;

    struct TestStore;

    #[async_trait]
    impl Store<i32, Arc<RwLock<String>>> for TestStore {
        async fn fetch(&self, _key: &i32) -> Arc<RwLock<String>> {
            println!("Fetching...");
            Arc::new(RwLock::new(String::from("Hello")))
        }

        async fn update(&self, _key: Arc<i32>, _value: Arc<RwLock<String>>) {
            println!("Updating...");
        }
    }

    #[tokio::test]
    async fn it_works() {
        let cache = Cache::new(TestStore);

        let _v = cache.get(&12).await;
        let _v = cache.get(&12).await;

        cache.cache.invalidate(&12).await;
    }
}

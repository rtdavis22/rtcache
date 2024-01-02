use std::hash::Hash;
use std::sync::Arc;

use async_trait::async_trait;
use moka::future::FutureExt;
use moka::notification::ListenerFuture;

pub struct Cache<K, V> {
    cache: moka::future::Cache<K, Arc<V>>,
    store: Arc<dyn Store<K, V>>,
    //background_task: tokio::task::JoinHandle<()>,
}

impl<K, V> Cache<K, V>
where
    K: Hash + Eq + Send + Sync + 'static,
    V: std::fmt::Debug + Clone + Send + Sync + 'static,
{
    pub async fn new(store: impl Store<K, V> + Send + Sync + 'static) -> Self {
        let store = Arc::new(store);

        let store_clone = store.clone();
        let eviction_listener = move |k: Arc<K>, mut v: Arc<V>, _cause| -> ListenerFuture {
            let store = store_clone.clone();
            async move {
                // wait until ref count is 1?
                tokio::spawn(async move {
                    loop {
                        println!(
                            "[Eviction Listener] Strong count: {}",
                            Arc::strong_count(&v)
                        );
                        match Arc::try_unwrap(v) {
                            Ok(v) => {
                                store.update(k, v).await;
                                break;
                            }
                            Err(arc) => {
                                v = arc;
                            }
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
                    }
                })
                .await
                .unwrap();
            }
            .boxed()
        };

        let cache: moka::future::Cache<K, Arc<V>> =
            moka::future::CacheBuilder::<K, Arc<V>, _>::new(10_000)
                .async_eviction_listener(eviction_listener)
                //.support_invalidation_closures()
                .build();

        /*
        let cache_clone = cache.clone();
        tokio::spawn(async move {
            loop {
                println!("Invalidating...");
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                cache_clone
                    .invalidate_entries_if(move |_k, v| Arc::strong_count(v) == 1)
                    .unwrap();
            }
        });
        */

        //tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        Self {
            cache,
            store,
            //background_task,
        }
    }

    pub async fn get(&self, key: &K) -> Arc<V>
    where
        K: ToOwned<Owned = K>,
    {
        self.cache
            .get_with_by_ref(key, async move { Arc::new(self.store.fetch(key).await) })
            .await
    }
}

#[async_trait]
pub trait Store<K, V> {
    async fn fetch(&self, key: &K) -> V;
    async fn update(&self, key: Arc<K>, value: V);
}

// what if entry evicted from cache and there's still a cloned arc out there?
// should we stop the eviction?
// maybe the cache shouldn't evict at all and we should do
// all invalidations manually?

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::RwLock;

    struct RwLockWrapper<T> {
        lock: RwLock<T>,
    }

    struct TestArcStore;

    #[async_trait]
    impl Store<i32, String> for TestArcStore {
        async fn fetch(&self, _key: &i32) -> String {
            println!("Fetching...");
            String::from("Hello")
        }

        async fn update(&self, _key: Arc<i32>, _value: String) {
            println!("Updating...");
            // if list is dirty, update store
        }
    }

    #[tokio::test]
    async fn it_works() {
        let cache = Cache::new(TestArcStore).await;

        {
            let v = cache.get(&12).await;
            println!("[1] Strong count: {}", Arc::strong_count(&v));
        }

        if let Some(v) = cache.cache.remove(&12).await {
            cache.cache.run_pending_tasks().await;
            println!("Strong count: {}", Arc::strong_count(&v));
        }

        //let v = cache.get(&12).await;
        //drop(v);

        cache.cache.invalidate_all();
        cache.cache.run_pending_tasks().await;

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        /*
                //for i in 1..10 {
                //    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                //    let _ = cache.get(&12).await;
                //}

                //cache.cache.invalidate(&12).await;
                //
                let cache2 = moka::future::Cache::new(10);
                cache2.insert(10, Arc::new(String::from("hello"))).await;

                if let Some(v) = cache2.remove(&10).await {
                    cache2.run_pending_tasks().await;
                    println!("Strong count: {}", Arc::strong_count(&v));
                }
        */
    }
}

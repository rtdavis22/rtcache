# rtcache

A read-through/write-back caching library.

To create a cache, first define a store and implement the `Store` trait:

```rust
struct TestStore {
    data: Arc<Mutex<HashMap<i32, String>>>,
}

impl TestStore {
    fn new(data: Arc<Mutex<HashMap<i32, String>>>) -> Self {
        Self { data }
    }
}

#[async_trait]
impl Store<i32, Mutex<String>> for TestStore {
    async fn fetch(&self, key: &i32) -> anyhow::Result<Mutex<String>> {
        if let Some(value) = self.data.lock().await.get(key) {
            Ok(Mutex::new(value.clone()))
        } else {
            Err(anyhow::anyhow!("Key not found"))
        }
    }

    // Optional
    async fn update(&self, key: i32, value: Mutex<String>) {
        self.data.lock().await.insert(key, value.into_inner());
    }
}
```

Then, initialize the cache:

```rust
let mut data = HashMap::new();
data.insert(1, String::from("Hello"));
let data = Arc::new(Mutex::new(data));

let mut cache = Cache::new(TestStore::new(data.clone()), Duration::from_secs(60)).await;
```

Use `Cache::get` to access values in the cache.

```rust
// The store's `fetch` function will be used if this key isn't in the cache already.
let value = cache.get(1).await.unwrap();
```

Values may be modified using [interior mutability](https://doc.rust-lang.org/book/ch15-05-interior-mutability.html):

```rust
*value.lock().await = String::from("World");
```

The `Store`'s `update` function is called when a key/value pair is evicted from the cache. This is a good place to update the cache's backing store in case there were any modifications to the value.

See examples/examples.rb for more.

**Important note: The cache internally stores values in `Arc`s and `Cache::get` returns a cloned pointer. If an `Arc<V>` has a reference count more than one, the `Arc` is in use outside of the cache and the key/value pair will not be evicted from the cache.**

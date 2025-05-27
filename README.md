# Thru

A high-performance, thread-safe caching library for Rust with built-in TTL support and web UI monitoring.

## Features

- 🚀 **High Performance**: Built on Tokio for async/await support
- 🔒 **Thread-Safe**: Fully concurrent and safe for multi-threaded environments
- ⏱️ **TTL Support**: Automatic cache entry expiration
- 📊 **Web UI**: Built-in monitoring interface
- 🔄 **Automatic Pruning**: Background task for cache maintenance
- 🎯 **Type-Safe**: Generic implementation supporting any key-value types
- 🔌 **Extensible**: Customizable storage backends via the `Store` trait

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
thru = "0.1.0"
```

## Quick Start

```rust
use std::time::Duration;
use thru::{Cache, Store};
use anyhow::Result;

// Implement your storage backend
struct MyStore;

#[async_trait::async_trait]
impl Store<String, String> for MyStore {
    async fn fetch(&self, key: &String) -> Result<String> {
        // Implement your fetch logic
        Ok(format!("Value for {}", key))
    }

    async fn update(&self, key: String, value: String) {
        // Implement your update logic
    }
}

#[tokio::main]
async fn main() {
    // Create a new cache with 1 hour TTL
    let cache = Cache::new(MyStore, Duration::from_secs(3600)).await;

    // Get a value (will fetch if not in cache)
    let value = cache.get("my_key".to_string()).await.unwrap();
    println!("Value: {}", value);
}
```

## Key Concepts

### Cache Entry States

The cache maintains entries in one of three states:

- `Fetching`: Entry is currently being fetched
- `FetchFailed`: Previous fetch attempt failed
- `Node`: Valid cache entry with value

### Features in Detail

1. **Automatic TTL**: Cache entries are automatically removed after their TTL expires
2. **Concurrent Access**: Multiple threads can safely access the cache simultaneously
3. **Web UI**: Monitor cache statistics and performance through a built-in web interface
4. **Background Pruning**: Automatic cleanup of expired entries
5. **Error Handling**: Robust error handling with `anyhow` integration

## API Reference

### Main Types

- `Cache<K, V>`: The main cache type
- `Store<K, V>`: Trait for implementing custom storage backends
- `GetError`: Error type for cache retrieval failures

### Key Methods

- `Cache::new(store, ttl)`: Create a new cache instance
- `Cache::get(key)`: Retrieve a value (fetches if not cached)
- `Cache::insert(key, value)`: Manually insert a value
- `Cache::remove(key)`: Remove a value from the cache
- `Cache::try_evict(key)`: Attempt to evict a specific entry
- `Cache::evict_all_sync()`: Remove all entries from the cache

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.

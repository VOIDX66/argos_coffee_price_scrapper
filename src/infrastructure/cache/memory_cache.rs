use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::domain::entities::coffee_market_indicators::CoffeeMarketIndicators;
use crate::domain::traits::cache_repository::CacheRepository;

struct CacheEntry {
    value: CoffeeMarketIndicators,
    expires_at: Instant,
}

#[derive(Default)]
pub struct MemoryCache {
    entries: RwLock<HashMap<String, CacheEntry>>,
}

impl MemoryCache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl CacheRepository<CoffeeMarketIndicators> for MemoryCache {
    async fn get(&self, key: &str) -> Result<Option<CoffeeMarketIndicators>> {
        let guard = self.entries.read().await;
        if let Some(entry) = guard.get(key) {
            if Instant::now() < entry.expires_at {
                return Ok(Some(entry.value.clone()));
            }
        }
        Ok(None)
    }

    async fn set(&self, key: &str, value: &CoffeeMarketIndicators, ttl_seconds: u64) -> Result<()> {
        let mut guard = self.entries.write().await;
        guard.insert(
            key.to_string(),
            CacheEntry {
                value: value.clone(),
                expires_at: Instant::now() + Duration::from_secs(ttl_seconds),
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[tokio::test]
    async fn test_memory_cache_set_and_get() {
        let cache = MemoryCache::new();
        let date = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let indicators = CoffeeMarketIndicators::new(date, 2_220_000.0, 75_000.0, 2.48, 4150.0, 2_180_000.0);

        cache.set("coffee:key", &indicators, 60).await.unwrap();
        let retrieved = cache.get("coffee:key").await.unwrap();
        assert_eq!(retrieved, Some(indicators));
    }

    #[tokio::test]
    async fn test_memory_cache_expired_returns_none() {
        let cache = MemoryCache::new();
        let date = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let indicators = CoffeeMarketIndicators::new(date, 2_220_000.0, 75_000.0, 2.48, 4150.0, 2_180_000.0);

        cache.set("coffee:expired", &indicators, 0).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        let retrieved = cache.get("coffee:expired").await.unwrap();
        assert_eq!(retrieved, None);
    }
}

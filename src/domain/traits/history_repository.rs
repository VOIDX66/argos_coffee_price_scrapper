use anyhow::Result;
use async_trait::async_trait;

use crate::domain::entities::coffee_market_indicators::CoffeeMarketIndicators;

#[async_trait]
pub trait HistoryRepository: Send + Sync {
    async fn save_daily(&self, record: &CoffeeMarketIndicators) -> Result<()>;
    async fn get_recent(&self, limit: usize) -> Result<Vec<CoffeeMarketIndicators>>;
}

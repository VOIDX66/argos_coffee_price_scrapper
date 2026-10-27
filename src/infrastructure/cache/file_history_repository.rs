//! Persistent JSON + In-Memory History Repository for Daily Coffee Indicators.
//! Stores one record per `publication_date`, ordered from newest to oldest.

use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::sync::RwLock;
use tracing::warn;

use crate::domain::entities::coffee_market_indicators::CoffeeMarketIndicators;
use crate::domain::traits::history_repository::HistoryRepository;

pub struct FileHistoryRepository {
    records: RwLock<Vec<CoffeeMarketIndicators>>,
    file_path: PathBuf,
}

impl FileHistoryRepository {
    pub fn new(file_path: PathBuf) -> Self {
        let initial = load_from_disk(&file_path).unwrap_or_default();
        Self {
            records: RwLock::new(initial),
            file_path,
        }
    }

    pub fn default_persistent() -> Self {
        let path = std::env::var("COFFEE_HISTORY_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| resolve_default_history_path());
        Self::new(path)
    }
}

#[async_trait]
impl HistoryRepository for FileHistoryRepository {
    async fn save_daily(&self, record: &CoffeeMarketIndicators) -> Result<()> {
        let snapshot = {
            let mut guard = self.records.write().await;
            if let Some(existing) = guard
                .iter_mut()
                .find(|r| r.publication_date == record.publication_date)
            {
                *existing = record.clone();
            } else {
                guard.push(record.clone());
            }
            guard.sort_by(|a, b| b.publication_date.cmp(&a.publication_date));
            if guard.len() > 365 {
                guard.truncate(365);
            }
            guard.clone()
        };

        save_to_disk(&self.file_path, &snapshot);
        Ok(())
    }

    async fn get_recent(&self, limit: usize) -> Result<Vec<CoffeeMarketIndicators>> {
        let guard = self.records.read().await;
        let count = limit.min(guard.len());
        Ok(guard[..count].to_vec())
    }
}

fn resolve_default_history_path() -> PathBuf {
    let vps_dir = PathBuf::from("/opt/apps/argos");
    if vps_dir.is_dir() {
        return vps_dir.join("coffee_history.json");
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("argos").join("coffee_history.json");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("argos")
                .join("coffee_history.json");
        }
    }
    std::env::temp_dir().join("argos_coffee_history.json")
}

fn load_from_disk(path: &PathBuf) -> Option<Vec<CoffeeMarketIndicators>> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut parsed: Vec<CoffeeMarketIndicators> = serde_json::from_str(&content).ok()?;
    parsed.sort_by(|a, b| b.publication_date.cmp(&a.publication_date));
    Some(parsed)
}

fn save_to_disk(path: &PathBuf, records: &[CoffeeMarketIndicators]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json_str) = serde_json::to_string_pretty(records) {
        if let Err(e) = std::fs::write(path, json_str) {
            warn!("Failed to persist coffee history to {:?}: {e}", path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[tokio::test]
    async fn test_file_history_repository_upserts_and_sorts_by_date() {
        let tmp_path = std::env::temp_dir().join(format!(
            "test_coffee_history_{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&tmp_path);

        let repo = FileHistoryRepository::new(tmp_path.clone());
        let d1 = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();

        repo.save_daily(&CoffeeMarketIndicators::new(
            d1,
            2_000_000.0,
            70_000.0,
            295.0,
            3100.0,
            0.0,
        ))
        .await
        .unwrap();

        repo.save_daily(&CoffeeMarketIndicators::new(
            d2,
            2_100_000.0,
            72_000.0,
            300.95,
            3130.0,
            0.0,
        ))
        .await
        .unwrap();

        // Upsert d1 with updated price
        repo.save_daily(&CoffeeMarketIndicators::new(
            d1,
            2_050_000.0,
            70_000.0,
            295.0,
            3100.0,
            0.0,
        ))
        .await
        .unwrap();

        let recent = repo.get_recent(10).await.unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].publication_date, d2);
        assert_eq!(recent[1].publication_date, d1);
        assert_eq!(recent[1].internal_price_cop, 2_050_000.0);

        let _ = std::fs::remove_file(&tmp_path);
    }
}

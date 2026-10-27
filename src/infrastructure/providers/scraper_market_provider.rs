use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use chrono::NaiveDate;
use reqwest::{Client, header};
use scraper::{Html, Selector};
use std::sync::Arc;
use std::time::Duration;

use crate::domain::entities::coffee_market_indicators::CoffeeMarketIndicators;
use crate::domain::traits::coffee_market_provider::CoffeeMarketProvider;

const DEFAULT_SCRAPER_URL: &str = "https://federaciondecafeteros.org/publicaciones/";

struct ScraperSelectors {
    menu_item: Selector,
    title: Selector,
    content: Selector,
}

pub struct ScraperCoffeeMarketProvider {
    client: Client,
    selectors: Arc<ScraperSelectors>,
    target_url: String,
}

impl Default for ScraperCoffeeMarketProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ScraperCoffeeMarketProvider {
    pub fn new() -> Self {
        let target_url = std::env::var("COFFEE_SCRAPER_URL")
            .unwrap_or_else(|_| DEFAULT_SCRAPER_URL.to_string());
        Self::with_url(target_url)
    }

    pub fn with_url(target_url: String) -> Self {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            ),
        );

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(14))
            .build()
            .unwrap_or_else(|_| Client::new());

        let selectors = ScraperSelectors {
            menu_item: Selector::parse(".e-n-menu-item").expect("Invalid menu item selector"),
            title: Selector::parse(".e-n-menu-title-text").expect("Invalid title selector"),
            content: Selector::parse(".e-n-menu-content").expect("Invalid content selector"),
        };

        Self {
            client,
            selectors: Arc::new(selectors),
            target_url,
        }
    }

    pub fn parse_money(value: &str) -> Result<f64> {
        let cleaned: String = value
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == ',')
            .collect();

        let normalized = cleaned.replace(',', ".");

        normalized
            .parse::<f64>()
            .with_context(|| format!("No se pudo parsear el valor numérico: {}", value))
    }

    pub fn parse_html_indicators(&self, html: &str) -> Result<CoffeeMarketIndicators> {
        let document = Html::parse_document(html);

        let mut publication_date = None;
        let mut internal_price = None;
        let mut pasilla = None;
        let mut ny_price = None;
        let mut exchange_rate = None;
        let mut mecic = None;

        for item in document.select(&self.selectors.menu_item) {
            if let Some(title_elem) = item.select(&self.selectors.title).next() {
                let full_title = title_elem.text().collect::<Vec<_>>().join(" ");
                let full_title = full_title.trim();

                let parts: Vec<&str> = full_title.split(':').collect();
                if parts.len() >= 2 {
                    let name = parts[0].trim();
                    let value = parts[1..].join(":").trim().to_string();

                    match name {
                        n if n.contains("Precio interno") => {
                            internal_price = Some(Self::parse_money(&value)?)
                        }
                        n if n.contains("Bolsa de NY") => {
                            ny_price = Some(Self::parse_money(&value)?)
                        }
                        n if n.contains("Tasa de cambio") => {
                            exchange_rate = Some(Self::parse_money(&value)?)
                        }
                        n if n.contains("MeCIC") => mecic = Some(Self::parse_money(&value)?),
                        n if n.contains("Pasilla") => pasilla = Some(Self::parse_money(&value)?),
                        _ => {}
                    }
                }
            }

            if publication_date.is_none() {
                if let Some(content_elem) = item.select(&self.selectors.content).next() {
                    let content_text = content_elem.text().collect::<Vec<_>>().join(" ");

                    if let Some(idx) = content_text.find("Fecha:") {
                        let date_str = content_text[idx + 6..]
                            .trim()
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .trim_matches(|c: char| !c.is_ascii_digit() && c != '-');
                        if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                            publication_date = Some(date);
                        }
                    }
                }
            }
        }

        let date =
            publication_date.ok_or_else(|| anyhow!("No se encontró la fecha de publicación"))?;
        let price = internal_price.ok_or_else(|| anyhow!("No se encontró el precio interno"))?;

        Ok(CoffeeMarketIndicators::new(
            date,
            price,
            pasilla.unwrap_or(0.0),
            ny_price.unwrap_or(0.0),
            exchange_rate.unwrap_or(0.0),
            mecic.unwrap_or(0.0),
        ))
    }
}

#[async_trait]
impl CoffeeMarketProvider for ScraperCoffeeMarketProvider {
    async fn get_market_indicators(&self) -> Result<CoffeeMarketIndicators> {
        let response = self
            .client
            .get(&self.target_url)
            .send()
            .await
            .context("Error al conectar con la web de la Federación Nacional de Cafeteros")?
            .error_for_status()
            .context("Respuesta HTTP no exitosa de la Federación Nacional de Cafeteros")?
            .text()
            .await?;

        self.parse_html_indicators(&response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_money_formats() {
        assert_eq!(
            ScraperCoffeeMarketProvider::parse_money("$2.220.000").unwrap(),
            2_220_000.0
        );
        assert_eq!(
            ScraperCoffeeMarketProvider::parse_money("248,50 US¢/lb").unwrap(),
            248.50
        );
        assert_eq!(
            ScraperCoffeeMarketProvider::parse_money("$4.185,25").unwrap(),
            4185.25
        );
        assert!(ScraperCoffeeMarketProvider::parse_money("sin numero").is_err());
    }

    #[test]
    fn test_parse_html_indicators_complete() {
        let sample_html = r#"
        <html>
        <body>
            <div class="e-n-menu-item">
                <span class="e-n-menu-title-text">Precio interno de referencia: $2.450.000</span>
                <div class="e-n-menu-content">Fecha: 2026-10-26</div>
            </div>
            <div class="e-n-menu-item">
                <span class="e-n-menu-title-text">Pasilla de finca: $75.000</span>
            </div>
            <div class="e-n-menu-item">
                <span class="e-n-menu-title-text">Bolsa de NY: 252,35 US¢/lb</span>
            </div>
            <div class="e-n-menu-item">
                <span class="e-n-menu-title-text">Tasa de cambio: $4.210,50</span>
            </div>
            <div class="e-n-menu-item">
                <span class="e-n-menu-title-text">MeCIC: $2.410.000</span>
            </div>
        </body>
        </html>
        "#;

        let provider = ScraperCoffeeMarketProvider::new();
        let indicators = provider.parse_html_indicators(sample_html).unwrap();

        assert_eq!(
            indicators.publication_date,
            NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()
        );
        assert_eq!(indicators.internal_price_cop, 2_450_000.0);
        assert_eq!(indicators.pasilla_cop, 75_000.0);
        assert_eq!(indicators.ny_price_usd, 252.35);
        assert_eq!(indicators.exchange_rate_cop_usd, 4210.50);
        assert_eq!(indicators.mecic_cop, 2_410_000.0);
    }

    #[test]
    fn test_parse_html_indicators_missing_fields_returns_error() {
        let provider = ScraperCoffeeMarketProvider::new();
        assert!(provider.parse_html_indicators("").is_err());
        assert!(provider
            .parse_html_indicators("<html><body>Sin indicadores</body></html>")
            .is_err());
    }
}


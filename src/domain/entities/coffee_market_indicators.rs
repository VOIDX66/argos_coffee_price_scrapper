// coffer_market_indicators.rs
/*
Estructura para representar los indicadores del mercado del café
*/
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CoffeeMarketIndicators {
    pub publication_date: NaiveDate,
    pub internal_price_cop: f64,
    pub pasilla_cop: f64,
    pub ny_price_usd: f64,
    pub exchange_rate_cop_usd: f64,
    pub mecic_cop: f64,
}

impl CoffeeMarketIndicators {
    pub fn new(
        publication_date: NaiveDate,
        internal_price_cop: f64,
        pasilla_cop: f64,
        ny_price_usd: f64,
        exchange_rate_cop_usd: f64,
        mecic_cop: f64,
    ) -> Self {
        Self {
            publication_date,
            internal_price_cop,
            pasilla_cop,
            ny_price_usd,
            exchange_rate_cop_usd,
            mecic_cop,
        }
    }

    /// Formats a slice of daily coffee indicators into a compact, LLM-ready TOON string
    /// containing precomputed summary statistics (min, max, average, variation) and a daily table.
    pub fn format_history_toon(records: &[Self]) -> String {
        if records.is_empty() {
            return "resumen_historico{dias_registrados}:\n  0\nhistorial_diario[0]{fecha,precio_interno_cop,bolsa_ny_usc,trm_cop,pasilla_cop,mecic_cop}:".to_string();
        }

        let mut sorted = records.to_vec();
        sorted.sort_by(|a, b| b.publication_date.cmp(&a.publication_date));

        let latest = &sorted[0];
        let oldest = &sorted[sorted.len() - 1];

        let mut min_rec = latest;
        let mut max_rec = latest;
        let mut sum_cop = 0.0;

        for rec in &sorted {
            if rec.internal_price_cop < min_rec.internal_price_cop {
                min_rec = rec;
            }
            if rec.internal_price_cop > max_rec.internal_price_cop {
                max_rec = rec;
            }
            sum_cop += rec.internal_price_cop;
        }

        let avg_cop = sum_cop / (sorted.len() as f64);
        let variation_cop = latest.internal_price_cop - oldest.internal_price_cop;

        let mut out = format!(
            "resumen_historico{{dias_registrados,precio_actual_cop,fecha_actual,precio_min_cop,fecha_min,precio_max_cop,fecha_max,promedio_cop,variacion_periodo_cop}}:\n  {},{:.0},\"{}\",{:.0},\"{}\",{:.0},\"{}\",{:.0},{:+.0}\n",
            sorted.len(),
            latest.internal_price_cop,
            latest.publication_date,
            min_rec.internal_price_cop,
            min_rec.publication_date,
            max_rec.internal_price_cop,
            max_rec.publication_date,
            avg_cop,
            variation_cop
        );

        out.push_str(&format!(
            "historial_diario[{}]{{fecha,precio_interno_cop,bolsa_ny_usc,trm_cop,pasilla_cop,mecic_cop}}:\n",
            sorted.len()
        ));

        for rec in &sorted {
            out.push_str(&format!(
                "  \"{}\",{:.0},{:.2},{:.2},{:.0},{:.0}\n",
                rec.publication_date,
                rec.internal_price_cop,
                rec.ny_price_usd,
                rec.exchange_rate_cop_usd,
                rec.pasilla_cop,
                rec.mecic_cop
            ));
        }

        out.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_history_toon_computes_min_max_avg_and_table() {
        let d1 = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let d3 = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();

        let records = vec![
            CoffeeMarketIndicators::new(d1, 2_000_000.0, 70_000.0, 290.0, 3100.0, 0.0),
            CoffeeMarketIndicators::new(d2, 2_250_000.0, 75_000.0, 310.5, 3150.0, 0.0),
            CoffeeMarketIndicators::new(d3, 2_100_000.0, 72_000.0, 300.95, 3130.0, 0.0),
        ];

        let toon = CoffeeMarketIndicators::format_history_toon(&records);
        assert!(toon.contains("resumen_historico{dias_registrados,precio_actual_cop,fecha_actual,precio_min_cop,fecha_min,precio_max_cop,fecha_max,promedio_cop,variacion_periodo_cop}:"));
        assert!(toon.contains("3,2100000,\"2026-10-26\",2000000,\"2026-10-20\",2250000,\"2026-10-23\",2116667,+100000"));
        assert!(toon.contains("historial_diario[3]{fecha,precio_interno_cop,bolsa_ny_usc,trm_cop,pasilla_cop,mecic_cop}:"));
        assert!(toon.contains("\"2026-10-26\",2100000,300.95,3130.00,72000,0"));
    }
}
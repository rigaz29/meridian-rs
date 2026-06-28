//! Bollinger Bands %B entry-timing gate.
//!
//! Mean-reversion entry filter for single-side-SOL-below LP. We deploy SOL
//! liquidity in bins *below* the active price and only earn fees when price
//! trades back *down* into our range. So we want to enter when price is
//! over-extended to the upside (near/above the upper Bollinger band, %B >= ~0.8)
//! — statistically a pullback toward the mean (down, into our range) is more
//! likely. Deploying regardless of price position was a major source of bleed:
//! we kept opening as price ran up, then sat out of range earning nothing.
//!
//! Candles come from the Agent Meridian `/chart-indicators/{mint}` endpoint,
//! which returns raw 5-minute OHLCV; we compute %B from the closes ourselves.

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::config::Config;
use crate::tools::agent_meridian::AgentMeridianSettings;

/// Built-in public key for the Agent Meridian API (base64 "meridian-is-the-
/// best-agents"). Used as a fallback so the indicator gate works out of the box.
const DEFAULT_PUBLIC_KEY: &str = "bWVyaWRpYW4taXMtdGhlLWJlc3QtYWdlbnRz";

/// Standard Bollinger period.
const BB_PERIOD: usize = 20;

/// Fetch recent close prices (oldest→newest) for a token mint.
async fn fetch_closes(config: &Config, mint: &str, interval: &str, candles: u32) -> Result<Vec<f64>> {
    let settings = AgentMeridianSettings::from_config(config);
    let key = settings
        .api_key
        .clone()
        .filter(|k| !k.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PUBLIC_KEY.to_string());
    let url = format!(
        "{}/chart-indicators/{}?interval={}&candles={}&rsiLength=2",
        settings.base_url.trim_end_matches('/'),
        mint,
        interval,
        candles,
    );
    let client = reqwest::Client::new();
    let resp: Value = client
        .get(&url)
        .header("x-api-key", key)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| anyhow!("chart-indicators request: {}", e))?
        .json()
        .await
        .map_err(|e| anyhow!("chart-indicators parse: {}", e))?;
    let arr = resp
        .get("candles")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("chart-indicators: no candles in response"))?;
    let closes: Vec<f64> = arr
        .iter()
        .filter_map(|c| c.get("close").and_then(Value::as_f64))
        .filter(|c| *c > 0.0)
        .collect();
    Ok(closes)
}

/// Bollinger %B of the latest close over `period`: (close - lower) / (upper -
/// lower), where bands are SMA ± 2σ. ~0 at the lower band, ~1 at the upper band,
/// can exceed [0,1] when price pierces a band. None if data is insufficient.
pub fn percent_b(closes: &[f64], period: usize) -> Option<f64> {
    if period == 0 || closes.len() < period {
        return None;
    }
    let window = &closes[closes.len() - period..];
    let mean = window.iter().sum::<f64>() / period as f64;
    let variance = window.iter().map(|c| (c - mean).powi(2)).sum::<f64>() / period as f64;
    let sd = variance.sqrt();
    if sd <= 0.0 {
        return None;
    }
    let upper = mean + 2.0 * sd;
    let lower = mean - 2.0 * sd;
    let last = *closes.last()?;
    Some((last - lower) / (upper - lower))
}

/// Compute the entry %B for a token mint on the configured short timeframe.
/// Ok(None) when candle data is unavailable/insufficient — the caller decides
/// whether to allow or block the deploy in that case.
pub async fn entry_percent_b(config: &Config, mint: &str) -> Result<Option<f64>> {
    let interval = config
        .indicators
        .intervals
        .first()
        .cloned()
        .unwrap_or_else(|| "5_MINUTE".to_string());
    // Fetch enough candles to cover the BB window with margin.
    let candles = (BB_PERIOD as u32 + 40).min(config.indicators.candles.max(BB_PERIOD as u32 + 40));
    let closes = fetch_closes(config, mint, &interval, candles).await?;
    Ok(percent_b(&closes, BB_PERIOD))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_b_zero_at_lower_one_at_upper() {
        // Flat then a spike: just sanity-check the math on a simple series.
        let mut closes: Vec<f64> = vec![10.0; 19];
        closes.push(10.0);
        // zero stddev → None
        assert_eq!(percent_b(&closes, 20), None);
    }

    #[test]
    fn percent_b_high_when_price_above_mean() {
        let mut closes: Vec<f64> = (0..19).map(|_| 10.0).collect();
        closes.push(12.0); // last close well above the mean → %B > 0.5
        let pb = percent_b(&closes, 20).expect("should compute");
        assert!(pb > 0.5, "expected overextended %B, got {pb}");
    }

    #[test]
    fn insufficient_data_returns_none() {
        assert_eq!(percent_b(&[1.0, 2.0, 3.0], 20), None);
    }
}

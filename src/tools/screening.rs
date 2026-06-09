use crate::config::types::ScreeningConfig;
use crate::models::pool::{CondensedPool, PoolDiscoveryResponse, PoolToken, RawPool};
use crate::tools::discord_signals::{merge_discord_signal_pools, DiscordSignalStore};
use crate::utils::logger::module::info;
use anyhow::{Context, Result};
use reqwest::Client;

const POOL_DISCOVERY_BASE: &str = "https://pool-discovery-api.datapi.meteora.ag";
const MIN_VOLATILITY_TIMEFRAME: &str = "30m";
const PVP_MIN_ACTIVE_TVL: f64 = 5_000.0;
const PVP_MIN_HOLDERS: u64 = 500;
const PVP_MIN_GLOBAL_FEES_SOL: f64 = 30.0;

static TIMEFRAME_MINUTES: &[(&str, u32)] = &[
    ("5m", 5),
    ("30m", 30),
    ("1h", 60),
    ("2h", 120),
    ("4h", 240),
    ("12h", 720),
    ("24h", 1440),
];

pub struct Screener {
    client: Client,
}

impl Screener {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Fetch pools from Meteora Pool Discovery API with filters
    pub async fn discover_pools(
        &self,
        s: &ScreeningConfig,
        page_size: u32,
    ) -> Result<Vec<RawPool>> {
        let now_ms = chrono::Utc::now().timestamp_millis() as f64;

        let mut filters = vec![
            "base_token_has_critical_warnings=false".to_string(),
            "quote_token_has_critical_warnings=false".to_string(),
            "base_token_has_high_single_ownership=false".to_string(),
            "pool_type=dlmm".to_string(),
            format!("base_token_market_cap>={}", s.min_mcap),
            format!("base_token_market_cap<={}", s.max_mcap),
            format!("base_token_holders>={}", s.min_holders),
            format!("volume>={}", s.min_volume),
            format!("tvl>={}", s.min_tvl),
            format!("dlmm_bin_step>={}", s.min_bin_step),
            format!("dlmm_bin_step<={}", s.max_bin_step),
            format!("fee_active_tvl_ratio>={}", s.min_fee_active_tvl_ratio),
            format!("base_token_organic_score>={}", s.min_organic),
            format!("quote_token_organic_score>={}", s.min_quote_organic),
        ];

        if s.exclude_high_supply_concentration {
            filters.push("base_token_has_high_supply_concentration=false".to_string());
        }
        if let Some(max_tvl) = s.max_tvl {
            filters.push(format!("tvl<={}", max_tvl));
        }
        if let Some(hours) = s.min_token_age_hours {
            filters.push(format!(
                "base_token_created_at<={}",
                now_ms - hours * 3_600_000.0
            ));
        }
        if let Some(hours) = s.max_token_age_hours {
            filters.push(format!(
                "base_token_created_at>={}",
                now_ms - hours * 3_600_000.0
            ));
        }
        if !s.allowed_launchpads.is_empty() {
            filters.push(format!(
                "base_token_launchpad=[{}]",
                s.allowed_launchpads.join(",")
            ));
        }

        let filter_str = filters.join("&&");
        let url = format!(
            "{}/pools?page_size={}&filter_by={}&timeframe={}&category={}",
            POOL_DISCOVERY_BASE,
            page_size,
            urlencoding::encode(&filter_str),
            urlencoding::encode(&s.timeframe),
            urlencoding::encode(&s.category),
        );

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Pool Discovery API request failed")?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!(
                "Pool Discovery API error: {}",
                resp.status()
            ));
        }

        let data: PoolDiscoveryResponse = resp.json().await?;
        let mut raw_pools = data.data.unwrap_or_default();
        if s.use_discord_signals {
            match DiscordSignalStore::load_default() {
                Ok(store) => {
                    let pending = store.pending_cloned();
                    if !pending.is_empty() {
                        info(
                            "screening",
                            &format!(
                                "Merging {} pending Discord signal(s) from {}",
                                pending.len(),
                                store.path.display()
                            ),
                        );
                    }
                    raw_pools = merge_discord_signal_pools(
                        raw_pools,
                        &pending,
                        s.discord_signal_mode.as_deref(),
                    );
                }
                Err(error) => info(
                    "screening",
                    &format!("Discord signal queue load failed: {error}"),
                ),
            }
        }
        Ok(raw_pools)
    }

    /// Get pool detail for a specific address
    pub async fn get_pool_detail(
        &self,
        pool_address: &str,
        timeframe: &str,
    ) -> Result<Option<RawPool>> {
        let url = format!(
            "{}/pools?page_size=1&filter_by={}&timeframe={}",
            POOL_DISCOVERY_BASE,
            urlencoding::encode(&format!("pool_address={}", pool_address)),
            urlencoding::encode(timeframe),
        );

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Pool detail API error: {}", resp.status()));
        }

        let data: PoolDiscoveryResponse = resp.json().await?;
        Ok(data.data.and_then(|v| v.into_iter().next()))
    }

    /// Apply volatility from a longer timeframe if needed
    pub async fn apply_volatility_timeframe(
        &self,
        pools: &mut [RawPool],
        source_tf: &str,
    ) -> Result<()> {
        let vol_tf = get_volatility_timeframe(source_tf);
        if vol_tf == source_tf {
            return Ok(());
        }

        let pool_addrs: Vec<String> = pools
            .iter()
            .filter_map(|p| p.pool_address.clone())
            .collect();

        for addr in pool_addrs {
            if let Ok(Some(detail)) = self.get_pool_detail(&addr, vol_tf).await {
                if let Some(pool) = pools
                    .iter_mut()
                    .find(|p| p.pool_address.as_deref() == Some(&addr))
                {
                    if let Some(v) = detail.volatility {
                        pool.volatility = Some(v);
                    }
                    if let Some(v) = detail.volume {
                        pool.volume = Some(v);
                    }
                }
            }
        }
        Ok(())
    }

    /// Hard-filter pools against screening thresholds (post-API filter)
    fn reject_reason(&self, pool: &RawPool, s: &ScreeningConfig) -> Option<String> {
        let base = pool.token_x.as_ref();
        let _quote = pool.token_y.as_ref();

        if pool.base_token_has_critical_warnings == Some(true) {
            return Some("critical warnings".into());
        }
        if pool.quote_token_has_critical_warnings == Some(true) {
            return Some("quote critical warnings".into());
        }
        if pool.base_token_has_high_single_ownership == Some(true) {
            return Some("high single ownership".into());
        }
        if pool.pool_type.as_deref() != Some("dlmm") {
            return Some("not dlmm".into());
        }

        let mcap = base.and_then(|b| b.market_cap).or(pool.base_token_mcap);
        let holders = pool.base_token_holders;
        let volatility = pool.volatility;
        let bin_step = pool.dlmm_bin_step;
        let fee_ratio = pool.fee_active_tvl_ratio;
        let organic = base
            .and_then(|b| b.organic_score)
            .or(pool.base_token_organic_score);

        if mcap.unwrap_or(0.0) < s.min_mcap {
            return Some(format!("mcap {} < min {}", mcap.unwrap_or(0.0), s.min_mcap));
        }
        if mcap.unwrap_or(f64::MAX) > s.max_mcap {
            return Some(format!("mcap {} > max {}", mcap.unwrap(), s.max_mcap));
        }
        if holders.unwrap_or(0) < s.min_holders {
            return Some(format!(
                "holders {} < min {}",
                holders.unwrap_or(0),
                s.min_holders
            ));
        }
        if pool.volume.unwrap_or(0.0) < s.min_volume {
            return Some("low volume".into());
        }
        if pool.tvl.unwrap_or(pool.active_tvl.unwrap_or(0.0)) < s.min_tvl {
            return Some("low tvl".into());
        }
        if let Some(max) = s.max_tvl {
            if pool.tvl.unwrap_or(0.0) > max {
                return Some(format!("tvl > max {}", max));
            }
        }
        if bin_step.unwrap_or(0) < s.min_bin_step {
            return Some(format!(
                "bin_step {} < min {}",
                bin_step.unwrap_or(0),
                s.min_bin_step
            ));
        }
        if bin_step.unwrap_or(u16::MAX) > s.max_bin_step {
            return Some(format!(
                "bin_step {} > max {}",
                bin_step.unwrap(),
                s.max_bin_step
            ));
        }
        if fee_ratio.unwrap_or(0.0) < s.min_fee_active_tvl_ratio {
            return Some("low fee/tvl".into());
        }
        if volatility.is_none() || volatility.unwrap_or(0.0) <= 0.0 {
            return Some("no volatility".into());
        }
        if organic.unwrap_or(0.0) < s.min_organic {
            return Some("low organic".into());
        }

        // Blocked launchpads
        let launchpad =
            base.and_then(|b| b.launchpad.as_deref().or(b.launchpad_platform.as_deref()));
        if let Some(lp) = launchpad {
            if s.blocked_launchpads
                .iter()
                .any(|b| b.eq_ignore_ascii_case(lp))
            {
                return Some(format!("blocked launchpad {}", lp));
            }
        }
        None
    }

    /// Condense a raw pool into token-optimized format for LLM
    fn condense(&self, pool: &RawPool) -> CondensedPool {
        let base = pool.token_x.as_ref();
        let base_mint = base.and_then(|b| b.address.clone()).unwrap_or_default();
        let base_symbol = base
            .and_then(|b| b.symbol.clone())
            .unwrap_or_else(|| "?".to_string());
        let base_organic = base
            .and_then(|b| b.organic_score)
            .or(pool.base_token_organic_score)
            .unwrap_or(0.0);
        let quote_organic = pool
            .token_y
            .as_ref()
            .and_then(|q| q.organic_score)
            .or(pool.quote_token_organic_score)
            .unwrap_or(0.0);
        let mcap = base
            .and_then(|b| b.market_cap)
            .or(pool.base_token_mcap)
            .unwrap_or(0.0);

        CondensedPool {
            name: pool
                .name
                .clone()
                .unwrap_or_else(|| pool.pool_address.clone().unwrap_or_default()),
            pool_address: pool.pool_address.clone().unwrap_or_default(),
            base: PoolToken {
                mint: base_mint,
                symbol: base_symbol,
                organic: base_organic,
                mcap: Some(mcap),
            },
            quote_organic,
            tvl: pool.tvl.or(pool.active_tvl).unwrap_or(0.0),
            volume: pool.volume.unwrap_or(0.0),
            fee_active_tvl_ratio: pool.fee_active_tvl_ratio.unwrap_or(0.0),
            volatility: pool.volatility.unwrap_or(0.0),
            bin_step: pool.dlmm_bin_step.unwrap_or(100),
            score: score_candidate(pool),
            holders: pool.base_token_holders.unwrap_or(0),
            mcap,
            fees_sol: pool.fee.unwrap_or(0.0),
            launchpad: base
                .and_then(|b| b.launchpad.as_deref().or(b.launchpad_platform.as_deref()))
                .map(String::from)
                .or_else(|| pool.base_token_launchpad.clone()),
            dev: None,
            bundlers_pct: None,
            top10_pct: None,
            discord_signal: pool
                .extra
                .get("discord_signal")
                .and_then(|value| value.as_bool())
                .filter(|value| *value),
            is_pvp: None,
            pvp_risk: None,
            pvp_symbol: None,
            pvp_rival_name: None,
            pvp_rival_mint: None,
            pvp_rival_pool: None,
            pvp_rival_tvl: None,
            pvp_rival_holders: None,
            pvp_rival_fees: None,
        }
    }

    /// Full screening pipeline: discover → filter → condense → score → sort
    pub async fn get_top_candidates(
        &self,
        s: &ScreeningConfig,
        limit: usize,
    ) -> Result<Vec<CondensedPool>> {
        let raw = self.discover_pools(s, 50).await?;
        info(
            "screening",
            &format!("Discovery returned {} raw pools", raw.len()),
        );

        let mut filtered: Vec<CondensedPool> = raw
            .iter()
            .filter(|p| self.reject_reason(p, s).is_none())
            .map(|p| self.condense(p))
            .collect();

        info(
            "screening",
            &format!("{} pools passed filters", filtered.len()),
        );

        filtered.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        filtered.truncate(limit);
        apply_pvp_risk_policy(&mut filtered, s);
        Ok(filtered)
    }
}

pub fn apply_pvp_risk_policy(candidates: &mut Vec<CondensedPool>, s: &ScreeningConfig) {
    if !s.avoid_pvp_symbols || candidates.is_empty() {
        return;
    }

    enrich_pvp_risk_from_candidate_set(candidates);

    if s.block_pvp_symbols {
        let before = candidates.len();
        candidates.retain(|candidate| candidate.is_pvp != Some(true));
        let removed = before.saturating_sub(candidates.len());
        if removed > 0 {
            info(
                "screening",
                &format!("PVP hard filter removed {removed} pool(s)"),
            );
        }
    }
}

fn enrich_pvp_risk_from_candidate_set(candidates: &mut [CondensedPool]) {
    let snapshot = candidates.to_vec();

    for candidate in candidates.iter_mut() {
        let symbol = normalize_symbol(&candidate.base.symbol);
        if symbol.is_empty() || candidate.base.mint.is_empty() {
            continue;
        }

        let rival = snapshot
            .iter()
            .filter(|rival| rival.pool_address != candidate.pool_address)
            .filter(|rival| rival.base.mint != candidate.base.mint)
            .filter(|rival| normalize_symbol(&rival.base.symbol) == symbol)
            .filter(|rival| is_meaningful_pvp_rival(rival))
            .max_by(|a, b| {
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        a.tvl
                            .partial_cmp(&b.tvl)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });

        if let Some(rival) = rival {
            candidate.is_pvp = Some(true);
            candidate.pvp_risk = Some("high".to_string());
            candidate.pvp_symbol = Some(symbol);
            candidate.pvp_rival_name = Some(rival.name.clone());
            candidate.pvp_rival_mint = Some(rival.base.mint.clone());
            candidate.pvp_rival_pool = Some(rival.pool_address.clone());
            candidate.pvp_rival_tvl = Some(rival.tvl.round());
            candidate.pvp_rival_holders = Some(rival.holders);
            candidate.pvp_rival_fees = Some((rival.fees_sol * 100.0).round() / 100.0);
        }
    }
}

fn is_meaningful_pvp_rival(pool: &CondensedPool) -> bool {
    pool.tvl >= PVP_MIN_ACTIVE_TVL
        && pool.holders >= PVP_MIN_HOLDERS
        && pool.fees_sol >= PVP_MIN_GLOBAL_FEES_SOL
}

fn normalize_symbol(symbol: &str) -> String {
    symbol.trim().to_uppercase()
}

fn score_candidate(pool: &RawPool) -> f64 {
    let fee_tvl = pool.fee_active_tvl_ratio.unwrap_or(0.0);
    let organic = pool
        .token_x
        .as_ref()
        .and_then(|t| t.organic_score)
        .or(pool.base_token_organic_score)
        .unwrap_or(0.0);
    let volume = pool.volume.unwrap_or(0.0);
    let holders = pool.base_token_holders.unwrap_or(0) as f64;
    fee_tvl * 1000.0 + organic * 10.0 + volume / 100.0 + holders / 100.0
}

fn get_volatility_timeframe(source: &str) -> &str {
    let source_min = TIMEFRAME_MINUTES
        .iter()
        .find(|(k, _)| *k == source)
        .map(|(_, v)| *v);
    let min_30m = 30u32;
    match source_min {
        Some(m) if m >= min_30m => source,
        _ => MIN_VOLATILITY_TIMEFRAME,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::pool::PoolToken;

    fn candidate(
        pool_address: &str,
        mint: &str,
        symbol: &str,
        score: f64,
        tvl: f64,
        holders: u64,
        fees_sol: f64,
    ) -> CondensedPool {
        CondensedPool {
            name: format!("{symbol}/SOL"),
            pool_address: pool_address.to_string(),
            base: PoolToken {
                mint: mint.to_string(),
                symbol: symbol.to_string(),
                organic: 75.0,
                mcap: Some(500_000.0),
            },
            quote_organic: 80.0,
            tvl,
            volume: 10_000.0,
            fee_active_tvl_ratio: 0.12,
            volatility: 0.5,
            bin_step: 100,
            score,
            holders,
            mcap: 500_000.0,
            fees_sol,
            launchpad: Some("meteora".to_string()),
            dev: None,
            bundlers_pct: None,
            top10_pct: None,
            discord_signal: None,
            is_pvp: None,
            pvp_risk: None,
            pvp_symbol: None,
            pvp_rival_name: None,
            pvp_rival_mint: None,
            pvp_rival_pool: None,
            pvp_rival_tvl: None,
            pvp_rival_holders: None,
            pvp_rival_fees: None,
        }
    }

    #[test]
    fn pvp_policy_marks_exact_symbol_rivals_with_high_risk_metadata() {
        let mut screening = crate::config::types::Config::default().screening;
        screening.avoid_pvp_symbols = true;
        screening.block_pvp_symbols = false;
        let mut candidates = vec![
            candidate("PoolAlpha", "MintAlpha", "MOON", 500.0, 20_000.0, 900, 41.0),
            candidate("PoolBeta", "MintBeta", "MOON", 450.0, 15_000.0, 800, 35.0),
            candidate(
                "PoolGamma",
                "MintGamma",
                "MOONSHOT",
                900.0,
                30_000.0,
                1_200,
                50.0,
            ),
        ];

        apply_pvp_risk_policy(&mut candidates, &screening);

        let alpha = candidates
            .iter()
            .find(|pool| pool.pool_address == "PoolAlpha")
            .unwrap();
        assert_eq!(alpha.is_pvp, Some(true));
        assert_eq!(alpha.pvp_risk.as_deref(), Some("high"));
        assert_eq!(alpha.pvp_symbol.as_deref(), Some("MOON"));
        assert_eq!(alpha.pvp_rival_mint.as_deref(), Some("MintBeta"));
        assert_eq!(alpha.pvp_rival_pool.as_deref(), Some("PoolBeta"));
        assert_eq!(alpha.pvp_rival_holders, Some(800));
        assert_eq!(alpha.pvp_rival_fees, Some(35.0));

        let gamma = candidates
            .iter()
            .find(|pool| pool.pool_address == "PoolGamma")
            .unwrap();
        assert_eq!(gamma.is_pvp, None);
    }

    #[test]
    fn pvp_policy_hard_blocks_flagged_candidates_when_configured() {
        let mut screening = crate::config::types::Config::default().screening;
        screening.avoid_pvp_symbols = true;
        screening.block_pvp_symbols = true;
        let mut candidates = vec![
            candidate("PoolAlpha", "MintAlpha", "MOON", 500.0, 20_000.0, 900, 41.0),
            candidate("PoolBeta", "MintBeta", "MOON", 450.0, 15_000.0, 800, 35.0),
            candidate("PoolSafe", "MintSafe", "SAFE", 300.0, 12_000.0, 650, 34.0),
        ];

        apply_pvp_risk_policy(&mut candidates, &screening);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].pool_address, "PoolSafe");
    }

    #[test]
    fn pvp_metadata_serializes_for_llm_candidate_context() {
        let mut pool = candidate("PoolAlpha", "MintAlpha", "MOON", 500.0, 20_000.0, 900, 41.0);
        pool.is_pvp = Some(true);
        pool.pvp_risk = Some("high".to_string());
        pool.pvp_rival_name = Some("Moon Rival".to_string());
        pool.pvp_rival_mint = Some("MintBeta".to_string());

        let json = serde_json::to_value(pool).expect("candidate serializes");

        assert_eq!(json["is_pvp"], true);
        assert_eq!(json["pvp_risk"], "high");
        assert_eq!(json["pvp_rival_name"], "Moon Rival");
        assert_eq!(json["pvp_rival_mint"], "MintBeta");
    }
}

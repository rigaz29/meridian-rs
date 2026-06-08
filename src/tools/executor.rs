use anyhow::Result;
use serde_json::Value;

use crate::config::Config;
use crate::llm::ToolCall;
use crate::state::positions::PositionState;
use crate::state::pool_memory::PoolMemoryStore;
use crate::tools::screening::Screener;

const PROTECTED_TOOLS: &[&str] = &["deploy_position", "close_position", "claim_fees", "swap_token"];
const ONCE_PER_SESSION: &[&str] = &["deploy_position", "swap_token", "close_position"];

pub struct ToolExecutor {
    pub screener: Screener,
    pub executed_once: Vec<String>,
}

impl ToolExecutor {
    pub fn new() -> Self {
        Self {
            screener: Screener::new(),
            executed_once: vec![],
        }
    }

    pub fn is_once_locked(&self, name: &str) -> bool {
        ONCE_PER_SESSION.contains(&name) && self.executed_once.contains(&name.to_string())
    }

    pub async fn execute(
        &mut self,
        call: &ToolCall,
        config: &Config,
        positions: &PositionState,
        pool_memory: &PoolMemoryStore,
    ) -> (String, bool) {
        let name = call.function.name.as_str();
        let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(Value::Object(serde_json::Map::new()));

        if self.is_once_locked(name) {
            return (format!("Tool {} already executed this session. Skipping.", name), true);
        }

        if PROTECTED_TOOLS.contains(&name) {
            if let Err(e) = self.run_safety_checks(name, &args, config).await {
                return (format!("SAFETY CHECK FAILED: {}", e), true);
            }
        }

        let result = self.dispatch(name, &args, config, positions, pool_memory).await;

        if ONCE_PER_SESSION.contains(&name) {
            self.executed_once.push(name.to_string());
        }

        match result {
            Ok(msg) => (msg, false),
            Err(e) => (format!("Tool {} error: {}", name, e), true),
        }
    }

    async fn run_safety_checks(&self, name: &str, args: &Value, config: &Config) -> Result<()> {
        match name {
            "deploy_position" => {
                let pool_addr = args["pool_address"].as_str().unwrap_or("");
                if pool_addr.is_empty() {
                    return Err(anyhow::anyhow!("pool_address required"));
                }
                let amount = args["amount_y"].as_f64().or(args["amount_sol"].as_f64()).unwrap_or(0.0);
                if amount <= 0.0 { return Err(anyhow::anyhow!("amount must be > 0")); }
                if amount > config.risk.max_deploy_amount {
                    return Err(anyhow::anyhow!("amount {} exceeds max {}", amount, config.risk.max_deploy_amount));
                }
                // Validate pool still passes thresholds
                if let Ok(Some(detail)) = self.screener.get_pool_detail(pool_addr, &config.screening.timeframe).await {
                    let tvl = detail.tvl.or(detail.active_tvl).unwrap_or(0.0);
                    if tvl < config.screening.min_tvl {
                        return Err(anyhow::anyhow!("Pool TVL below min"));
                    }
                    let vol = detail.volatility.unwrap_or(0.0);
                    if vol <= 0.0 {
                        return Err(anyhow::anyhow!("Pool volatility unusable"));
                    }
                } else {
                    return Err(anyhow::anyhow!("Could not verify pool before deploy"));
                }
                Ok(())
            }
            "close_position" => {
                if args["position_id"].as_str().unwrap_or("").is_empty() {
                    return Err(anyhow::anyhow!("position_id required"));
                }
                Ok(())
            }
            _ => Ok(())
        }
    }

    async fn dispatch(
        &self,
        name: &str,
        args: &Value,
        config: &Config,
        positions: &PositionState,
        pool_memory: &PoolMemoryStore,
    ) -> Result<String> {
        match name {
            "get_wallet_balance" => {
                Ok(r#"{"sol": 0, "usd": 0, "note": "wallet balance stub"}"#.to_string())
            }
            "get_my_positions" => {
                let active = positions.get_active();
                if active.is_empty() { return Ok("No active positions.".to_string()); }
                let mut lines = vec!["Active positions:".to_string()];
                for p in &active {
                    let sym = p.base_symbol.as_deref().unwrap_or(&p.base_mint[..8.min(p.base_mint.len())]);
                    let pool = &p.pool_address[..8.min(p.pool_address.len())];
                    lines.push(format!("- {} ({}) {:.3} SOL, bins [{}, {}], status: {:?}",
                        sym, pool, p.amount_sol, p.lower_bin, p.upper_bin, p.status));
                }
                Ok(lines.join("
"))
            }
            "get_top_candidates" => {
                let limit = args["limit"].as_u64().unwrap_or(3) as usize;
                match self.screener.get_top_candidates(&config.screening, limit).await {
                    Ok(candidates) => Ok(serde_json::to_string_pretty(&candidates)?),
                    Err(e) => Ok(format!("Screening error: {}", e)),
                }
            }
            "get_pool_memory" => Ok(pool_memory.get_summary_for_prompt()),
            "add_pool_note" => Ok("Pool note added (stub).".to_string()),
            "get_position_pnl" => Ok(r#"{"pnl_sol": 0, "fees_earned": 0, "note": "PnL stub"}"#.to_string()),
            "deploy_position" => {
                let pool = args["pool_address"].as_str().unwrap_or("");
                let amount = args["amount_y"].as_f64().or(args["amount_sol"].as_f64()).unwrap_or(0.0);
                let bins = args["bins_below"].as_i64().unwrap_or(20);
                Ok(format!(r#"{{"status": "deployed", "pool": "{}", "amount_sol": {}, "bins_below": {}, "note": "STUB -- needs DLMM SDK"}}"#, pool, amount, bins))
            }
            "close_position" => {
                let pid = args["position_id"].as_str().unwrap_or("");
                Ok(format!(r#"{{"status": "closed", "position_id": "{}", "note": "STUB"}}"#, pid))
            }
            "claim_fees" => {
                let pid = args["position_id"].as_str().unwrap_or("");
                Ok(format!(r#"{{"status": "claimed", "position_id": "{}", "note": "STUB"}}"#, pid))
            }
            "swap_token" => Ok(r#"{"status": "swapped", "note": "STUB -- needs Jupiter"}"#.to_string()),
            "search_pools" => {
                let q = args["query"].as_str().unwrap_or("");
                Ok(format!(r#"{{"pools": [], "query": "{}", "note": "search stub"}}"#, q))
            }
            "get_token_info" => {
                let mint = args["mint"].as_str().unwrap_or("");
                Ok(format!(r#"{{"mint": "{}", "note": "token info stub"}}"#, mint))
            }
            "get_token_holders" => Ok(r#"{"holders": [], "note": "holders stub"}"#.to_string()),
            "get_token_narrative" => Ok(r#"{"narrative": "none", "note": "narrative stub"}"#.to_string()),
            "check_smart_wallets_on_pool" => Ok(r#"{"smart_wallets": [], "note": "stub"}"#.to_string()),
            "get_active_bin" => {
                let pool = args["pool_address"].as_str().unwrap_or("");
                Ok(format!(r#"{{"bin_id": 0, "price": 0, "pool": "{}", "note": "active bin stub"}}"#, pool))
            }
            "discover_pools" => {
                match self.screener.discover_pools(&config.screening, 50).await {
                    Ok(pools) => Ok(format!(r#"{{"count": {}, "note": "raw discovery"}}"#, pools.len())),
                    Err(e) => Ok(format!("Discovery error: {}", e)),
                }
            }
            _ => Ok(format!("Unknown tool: {}", name)),
        }
    }
}

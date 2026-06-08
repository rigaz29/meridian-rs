use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::Config;

/// Path to the Meridian JS CLI — used for on-chain operations
/// (deploy, close, claim) that require the Meteora DLMM SDK.
fn meridian_cli() -> String {
    std::env::var("MERIDIAN_CLI").unwrap_or_else(|_| "/root/meridian/cli.js".to_string())
}

/// Execute a Meridian JS CLI command and parse JSON output.
async fn cli_exec(args: &[&str]) -> Result<serde_json::Value> {
    let output = tokio::process::Command::new("node")
        .arg(meridian_cli())
        .args(args)
        .output()
        .await
        .map_err(|e| anyhow!("Failed to spawn meridian CLI: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        anyhow::bail!("CLI error (exit {}): {}", output.status.code().unwrap_or(-1), stderr);
    }

    serde_json::from_str(&stdout)
        .map_err(|e| anyhow!("Failed to parse CLI JSON output: {} — stdout: {}", e, &stdout[..stdout.len().min(200)]))
}

/// Meteora DLMM program ID.
pub const DLMM_PROGRAM_ID: &str = "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo";

/// Meteora DLMM Data API base URL.
const METEORA_DLMM_API: &str = "https://dlmm.datapi.meteora.ag";

/// Meteora Pool Discovery API base URL.
const METEORA_POOL_DISCOVERY_API: &str = "https://pool-discovery-api.datapi.meteora.ag";

/// Default slippage tolerance (10% in bps).
const DEFAULT_SLIPPAGE_BPS: u32 = 1000;

/// Default bin range for single-side SOL deploy.
const DEFAULT_BINS_BELOW: i64 = 20;

// ─── Response types ──────────────────────────────────────────────

/// Active bin information from the Meteora API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveBinInfo {
    pub bin_id: i32,
    pub price: f64,
    #[serde(default)]
    pub price_per_lamport: Option<String>,
}

/// Pool metadata from Meteora DLMM Data API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolMetadata {
    pub address: Option<String>,
    pub name: Option<String>,
    pub token_x: Option<PoolTokenInfo>,
    pub token_y: Option<PoolTokenInfo>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Pool token information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolTokenInfo {
    pub symbol: Option<String>,
    pub address: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Portfolio pool entry from Meteora.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioPool {
    pub pool_address: Option<String>,
    pub token_x: Option<String>,
    pub token_y: Option<String>,
    pub token_x_mint: Option<String>,
    pub token_y_mint: Option<String>,
    pub out_of_range: Option<bool>,
    pub positions_out_of_range: Option<Vec<String>>,
    pub list_positions: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Meteora portfolio API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioResponse {
    pub pools: Option<Vec<PortfolioPool>>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// PnL data from Meteora DLMM PnL API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionPnlData {
    pub position_address: Option<String>,
    pub address: Option<String>,
    pub position: Option<String>,
    pub lower_bin_id: Option<i32>,
    pub upper_bin_id: Option<i32>,
    pub pool_active_bin_id: Option<i32>,
    pub is_out_of_range: Option<bool>,
    pub pnl_usd: Option<f64>,
    pub pnl_sol: Option<f64>,
    pub pnl_pct_change: Option<f64>,
    pub pnl_sol_pct_change: Option<f64>,
    pub created_at: Option<f64>,
    pub fee_per_tvl_24h: Option<f64>,
    #[serde(default)]
    pub unrealized_pnl: Option<UnrealizedPnl>,
    #[serde(default)]
    pub all_time_fees: Option<AllTimeAmounts>,
    #[serde(default)]
    pub all_time_deposits: Option<AllTimeAmounts>,
    #[serde(default)]
    pub all_time_withdrawals: Option<AllTimeAmounts>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Unrealized PnL breakdown.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UnrealizedPnl {
    pub balances: Option<f64>,
    pub balances_sol: Option<f64>,
    #[serde(default)]
    pub unclaimed_fee_token_x: Option<TokenAmount>,
    #[serde(default)]
    pub unclaimed_fee_token_y: Option<TokenAmount>,
}

/// Token amount with USD value.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenAmount {
    pub amount_sol: Option<f64>,
    pub usd: Option<f64>,
}

/// All-time fee/deposit/withdrawal amounts.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AllTimeAmounts {
    pub total: Option<AllTimeTotal>,
}

/// All-time total breakdown.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AllTimeTotal {
    pub sol: Option<f64>,
    pub usd: Option<f64>,
}

/// PnL API response for a pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PnlPoolResponse {
    pub positions: Option<Vec<PositionPnlData>>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Position data returned to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionData {
    pub position: String,
    pub pool: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pair: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_mint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower_bin: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper_bin: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_bin: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_range: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unclaimed_fees_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_value_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_per_tvl_24h: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_minutes: Option<i64>,
}

/// My positions result returned to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyPositionsResult {
    pub wallet: Option<String>,
    pub total_positions: usize,
    pub positions: Vec<PositionData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Deploy position result returned to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_range: Option<BinRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_range: Option<PriceRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_step: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_fee: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wide_range: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_y: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Bin range info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinRange {
    pub min: i32,
    pub max: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<i32>,
}

/// Price range info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceRange {
    pub min: f64,
    pub max: f64,
}

/// Close position result returned to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_txs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_txs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_mint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Claim fees result returned to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_mint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Search pools result returned to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPoolsResult {
    pub query: String,
    pub total: usize,
    pub pools: Vec<SearchPoolEntry>,
}

/// Pool entry from search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPoolEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_step: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tvl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_24h: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_x: Option<SearchToken>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_y: Option<SearchToken>,
}

/// Token info in search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchToken {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mint: Option<String>,
}

/// Pool discovery API raw pool entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryPool {
    pub address: Option<String>,
    pub name: Option<String>,
    pub bin_step: Option<u16>,
    pub base_fee_percentage: Option<f64>,
    pub liquidity: Option<f64>,
    pub trade_volume_24h: Option<f64>,
    pub mint_x: Option<String>,
    pub mint_y: Option<String>,
    pub mint_x_symbol: Option<String>,
    pub mint_y_symbol: Option<String>,
    #[serde(default)]
    pub token_x: Option<PoolTokenInfo>,
    #[serde(default)]
    pub token_y: Option<PoolTokenInfo>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Pool discovery API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryResponse {
    pub data: Option<Vec<DiscoveryPool>>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// PnL data for a specific position (returned to LLM).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionPnlResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_value_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unclaimed_fee_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_time_fees_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_per_tvl_24h: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_range: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower_bin: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper_bin: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_bin: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_minutes: Option<i64>,
}

/// Wallet positions result for external wallets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletPositionsResult {
    pub wallet: String,
    pub total_positions: usize,
    pub positions: Vec<PositionData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ─── Helpers ─────────────────────────────────────────────────────

/// Round a number to `decimals` decimal places.
fn round(val: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (val * factor).round() / factor
}

/// Safe parse: returns 0.0 if value cannot be parsed.
fn safe_f64(val: &serde_json::Value) -> f64 {
    val.as_f64().unwrap_or(0.0)
}

/// Create an HTTP client with a reasonable timeout.
fn make_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .unwrap_or_default()
}

// ─── Public API ──────────────────────────────────────────────────

/// Get the active bin for a DLMM pool.
///
/// Calls the Meteora DLMM Data API to fetch the current active bin ID and price.
pub async fn get_active_bin(pool_address: &str) -> Result<ActiveBinInfo> {
    let pool_address = crate::tools::wallet::normalize_mint(pool_address);
    let client = make_client();
    let url = format!("{}/pools/{}", METEORA_DLMM_API, pool_address);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("Active bin API request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Active bin API error: {} {}", status, body));
    }

    let data: PoolMetadata = resp
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse active bin response: {}", e))?;

    // Extract active bin from the response
    // The pool metadata API returns bin information in the extra fields
    let active_bin_id = data
        .extra
        .get("active_bin")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;

    let active_price = data
        .extra
        .get("active_bin_price")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let price_per_lamport = data
        .extra
        .get("active_bin_price_per_lamport")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(ActiveBinInfo {
        bin_id: active_bin_id,
        price: active_price,
        price_per_lamport,
    })
}

/// Get all open DLMM positions for a wallet.
///
/// Calls the Meteora Portfolio API to discover open pools/positions,
/// then enriches with PnL data from the Meteora PnL API.
pub async fn get_my_positions(
    wallet_address: &str,
    _config: &Config,
) -> Result<MyPositionsResult> {
    let client = make_client();

    // ─── Fetch portfolio from Meteora ────────────────────────────
    let portfolio_url = format!(
        "{}/portfolio/open?user={}",
        METEORA_DLMM_API, wallet_address,
    );

    let portfolio_resp = client
        .get(&portfolio_url)
        .send()
        .await
        .map_err(|e| anyhow!("Portfolio API request failed: {}", e))?;

    if !portfolio_resp.status().is_success() {
        let status = portfolio_resp.status();
        let body = portfolio_resp.text().await.unwrap_or_default();
        return Ok(MyPositionsResult {
            wallet: Some(wallet_address.to_string()),
            total_positions: 0,
            positions: vec![],
            error: Some(format!("Portfolio API error: {} {}", status, body)),
        });
    }

    let portfolio: PortfolioResponse = portfolio_resp
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse portfolio response: {}", e))?;

    let pools = portfolio.pools.unwrap_or_default();
    tracing::info!("get_my_positions: found {} pool(s) with open positions", pools.len());

    // ─── Fetch PnL data for all pools in parallel ────────────────
    let mut all_positions = Vec::new();

    for pool in &pools {
        let pool_addr = pool.pool_address.as_deref().unwrap_or("");
        if pool_addr.is_empty() {
            continue;
        }

        let position_addresses = pool.list_positions.as_ref()
            .map(|v| v.clone())
            .unwrap_or_default();

        if position_addresses.is_empty() {
            continue;
        }

        // Fetch PnL for this pool
        let pnl_url = format!(
            "{}/positions/{}/pnl?user={}&status=open&pageSize=100&page=1",
            METEORA_DLMM_API, pool_addr, wallet_address,
        );

        let pnl_map: HashMap<String, PositionPnlData> = match client
            .get(&pnl_url)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                resp.json::<PnlPoolResponse>()
                    .await
                    .map(|data| {
                        let positions = data.positions.unwrap_or_default();
                        let mut map = HashMap::new();
                        for p in positions {
                            let addr = p.position_address
                                .as_deref()
                                .or(p.address.as_deref())
                                .or(p.position.as_deref())
                                .unwrap_or("")
                                .to_string();
                            if !addr.is_empty() {
                                map.insert(addr, p);
                            }
                        }
                        map
                    })
                    .unwrap_or_default()
            }
            _ => HashMap::new(),
        };

        for pos_addr in &position_addresses {
            let pnl = pnl_map.get(pos_addr);
            let lower_bin = pnl.and_then(|p| p.lower_bin_id);
            let upper_bin = pnl.and_then(|p| p.upper_bin_id);
            let active_bin = pnl.and_then(|p| p.pool_active_bin_id);
            let in_range = pnl.map(|p| !p.is_out_of_range.unwrap_or(false));

            let unclaimed_fees = pnl.map(|p| {
                let x_fee = p.unrealized_pnl
                    .as_ref()
                    .and_then(|u| u.unclaimed_fee_token_x.as_ref())
                    .and_then(|t| t.usd)
                    .unwrap_or(0.0);
                let y_fee = p.unrealized_pnl
                    .as_ref()
                    .and_then(|u| u.unclaimed_fee_token_y.as_ref())
                    .and_then(|t| t.usd)
                    .unwrap_or(0.0);
                round(x_fee + y_fee, 4)
            });

            let total_value = pnl.map(|p| {
                round(
                    p.unrealized_pnl
                        .as_ref()
                        .and_then(|u| u.balances)
                        .unwrap_or(0.0),
                    4,
                )
            });

            let pnl_usd = pnl.map(|p| round(p.pnl_usd.unwrap_or(0.0), 4));
            let pnl_pct = pnl.and_then(|p| p.pnl_pct_change).map(|v| round(v, 2));

            let age_minutes = pnl.and_then(|p| {
                p.created_at.map(|ts| {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs_f64();
                    ((now - ts) / 60.0) as i64
                })
            });

            let pair = format!(
                "{}/{}",
                pool.token_x.as_deref().unwrap_or("?"),
                pool.token_y.as_deref().unwrap_or("?"),
            );

            all_positions.push(PositionData {
                position: pos_addr.clone(),
                pool: pool_addr.to_string(),
                pair: Some(pair),
                base_mint: pool.token_x_mint.clone(),
                lower_bin,
                upper_bin,
                active_bin,
                in_range,
                unclaimed_fees_usd: unclaimed_fees,
                total_value_usd: total_value,
                pnl_usd,
                pnl_pct,
                fee_per_tvl_24h: pnl.and_then(|p| p.fee_per_tvl_24h).map(|v| round(v, 2)),
                age_minutes,
            });
        }
    }

    let total = all_positions.len();
    Ok(MyPositionsResult {
        wallet: Some(wallet_address.to_string()),
        total_positions: total,
        positions: all_positions,
        error: None,
    })
}

/// Get PnL and fee information for a specific position.
///
/// Calls the Meteora DLMM PnL API for the given pool and position.
pub async fn get_position_pnl(
    pool_address: &str,
    position_address: &str,
    wallet_address: &str,
) -> Result<PositionPnlResult> {
    let pool_address = crate::tools::wallet::normalize_mint(pool_address);
    let position_address = crate::tools::wallet::normalize_mint(position_address);

    let client = make_client();
    let url = format!(
        "{}/positions/{}/pnl?user={}&status=open&pageSize=100&page=1",
        METEORA_DLMM_API, pool_address, wallet_address,
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("PnL API request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("PnL API error: {} {}", status, body));
    }

    let data: PnlPoolResponse = resp
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse PnL response: {}", e))?;

    let positions = data.positions.unwrap_or_default();
    let pos = positions
        .iter()
        .find(|p| {
            p.position_address.as_deref() == Some(&position_address)
                || p.address.as_deref() == Some(&position_address)
                || p.position.as_deref() == Some(&position_address)
        })
        .ok_or_else(|| anyhow!("Position not found in PnL API"))?;

    let unclaimed_value = pos
        .unrealized_pnl
        .as_ref()
        .map(|u| {
            let x_usd = u.unclaimed_fee_token_x.as_ref().and_then(|t| t.usd).unwrap_or(0.0);
            let y_usd = u.unclaimed_fee_token_y.as_ref().and_then(|t| t.usd).unwrap_or(0.0);
            round(x_usd + y_usd, 4)
        })
        .unwrap_or(0.0);

    let current_value = pos
        .unrealized_pnl
        .as_ref()
        .and_then(|u| u.balances)
        .unwrap_or(0.0);

    let all_time_fees = pos
        .all_time_fees
        .as_ref()
        .and_then(|f| f.total.as_ref())
        .and_then(|t| t.usd)
        .unwrap_or(0.0);

    let age_minutes = pos.created_at.map(|ts| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        ((now - ts) / 60.0) as i64
    });

    Ok(PositionPnlResult {
        pnl_usd: pos.pnl_usd.map(|v| round(v, 4)),
        pnl_pct: pos.pnl_pct_change.map(|v| round(v, 2)),
        current_value_usd: Some(round(current_value, 4)),
        unclaimed_fee_usd: Some(unclaimed_value),
        all_time_fees_usd: Some(round(all_time_fees, 4)),
        fee_per_tvl_24h: pos.fee_per_tvl_24h.map(|v| round(v, 2)),
        in_range: pos.is_out_of_range.map(|oor| !oor),
        lower_bin: pos.lower_bin_id,
        upper_bin: pos.upper_bin_id,
        active_bin: pos.pool_active_bin_id,
        age_minutes,
    })
}

/// Deploy a new DLMM position (place SOL into a pool).
///
/// Since the Meteora DLMM SDK crate is not available in Rust, this function
/// builds the transaction data that would be needed for deployment. In the
/// current implementation, it returns a placeholder indicating what would
/// be deployed, with the correct struct for integration with a future SDK.
pub async fn deploy_position(
    pool_address: &str,
    amount_sol: f64,
    bins_below: Option<i64>,
    bins_above: Option<i64>,
    strategy: Option<&str>,
    config: &Config,
) -> Result<DeployResult> {
    let _ = config;
    let pool_address = crate::tools::wallet::normalize_mint(pool_address);

    if std::env::var("DRY_RUN").ok().as_deref() == Some("true") {
        let bins_below_val = bins_below.unwrap_or(DEFAULT_BINS_BELOW);
        let bins_above_val = bins_above.unwrap_or(0);
        return Ok(DeployResult {
            success: false,
            position: None,
            pool: Some(pool_address),
            pool_name: None,
            bin_range: Some(BinRange {
                min: 0,
                max: 0,
                active: None,
            }),
            price_range: None,
            bin_step: None,
            base_fee: None,
            strategy: strategy.map(String::from),
            wide_range: Some(bins_below_val + bins_above_val > 69),
            amount_x: Some(0.0),
            amount_y: Some(amount_sol),
            txs: None,
            error: None,
            note: Some(format!(
                "DRY RUN — would deploy {:.4} SOL with {} bins below, {} bins above",
                amount_sol, bins_below_val, bins_above_val,
            )),
        });
    }

    let bins_below_val = bins_below.unwrap_or(DEFAULT_BINS_BELOW);
    let bins_above_val = bins_above.unwrap_or(0);
    let total_bins = bins_below_val + bins_above_val;
    let strategy_str = strategy.unwrap_or("spot");
    let is_wide_range = total_bins > 69;

    tracing::info!(
        "deploy_position: pool={} amount_sol={:.4} bins=[{}, {}] strategy={} wide_range={}",
        &pool_address[..8.min(pool_address.len())],
        amount_sol,
        bins_below_val,
        bins_above_val,
        strategy_str,
        is_wide_range,
    );

    // ─── Fetch active bin and pool info ──────────────────────────
    let active_bin = get_active_bin(&pool_address).await.unwrap_or(ActiveBinInfo {
        bin_id: 0,
        price: 0.0,
        price_per_lamport: None,
    });

    let min_bin_id = active_bin.bin_id - bins_below_val as i32;
    let max_bin_id = if bins_above_val > 0 {
        active_bin.bin_id + bins_above_val as i32
    } else {
        active_bin.bin_id
    };

    // Fetch pool metadata for additional info
    let pool_meta = get_pool_metadata(&pool_address).await;
    let bin_step = pool_meta
        .as_ref()
        .and_then(|m| m.extra.get("bin_step"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let base_fee = pool_meta
        .as_ref()
        .and_then(|m| m.extra.get("base_fee_percentage"))
        .and_then(|v| v.as_f64());

    // Execute deploy via Meridian JS CLI (uses @meteora-ag/dlmm SDK)
    let amount_str = format!("{:.4}", amount_sol);
    let bins_below_str = bins_below_val.to_string();
    let bins_above_str = bins_above_val.to_string();
    let mut cli_args: Vec<&str> = vec![
        "deploy",
        "--pool", &pool_address,
        "--amount", &amount_str,
        "--bins-below", &bins_below_str,
        "--bins-above", &bins_above_str,
        "--strategy", strategy_str,
    ];

    if std::env::var("DRY_RUN").ok().as_deref() == Some("true") {
        cli_args.push("--dry-run");
    }

    match cli_exec(&cli_args).await {
        Ok(result) => {
            let position_addr = result.get("position").and_then(|v| v.as_str()).map(String::from);
            let txs = result.get("txs").and_then(|v| v.as_str()).map(String::from);
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let error = if !success {
                result.get("error").and_then(|v| v.as_str()).map(String::from)
            } else {
                None
            };

            Ok(DeployResult {
                success,
                position: position_addr.clone(),
                pool: Some(pool_address),
                pool_name: pool_meta
                    .as_ref()
                    .and_then(|m| m.name.clone()),
                bin_range: Some(BinRange {
                    min: min_bin_id,
                    max: max_bin_id,
                    active: Some(active_bin.bin_id),
                }),
                price_range: None,
                bin_step,
                base_fee,
                strategy: Some(strategy_str.to_string()),
                wide_range: Some(is_wide_range),
                amount_x: Some(0.0),
                amount_y: Some(amount_sol),
                txs: txs.map(|t| vec![t]),
                error,
                note: result.get("note").and_then(|v| v.as_str()).map(String::from),
            })
        }
        Err(e) => {
            tracing::error!("deploy_position CLI failed: {}", e);
            Ok(DeployResult {
                success: false,
                position: None,
                pool: Some(pool_address),
                pool_name: pool_meta
                    .as_ref()
                    .and_then(|m| m.name.clone()),
                bin_range: Some(BinRange {
                    min: min_bin_id,
                    max: max_bin_id,
                    active: Some(active_bin.bin_id),
                }),
                price_range: None,
                bin_step,
                base_fee,
                strategy: Some(strategy_str.to_string()),
                wide_range: Some(is_wide_range),
                amount_x: Some(0.0),
                amount_y: Some(amount_sol),
                txs: None,
                error: Some(format!("CLI execution failed: {}", e)),
                note: None,
            })
        }
    }
}

/// Close a DLMM position and withdraw liquidity.
///
/// Since the Meteora DLMM SDK crate is not available in Rust, this function
/// builds the close transaction data. In the current implementation, it
/// returns a placeholder indicating what would be closed.
pub async fn close_position(
    position_address: &str,
    reason: Option<&str>,
    _config: &Config,
) -> Result<CloseResult> {
    let position_address = crate::tools::wallet::normalize_mint(position_address);

    if std::env::var("DRY_RUN").ok().as_deref() == Some("true") {
        return Ok(CloseResult {
            success: false,
            position: Some(position_address),
            pool: None,
            pool_name: None,
            claim_txs: None,
            close_txs: None,
            txs: None,
            pnl_usd: None,
            pnl_pct: None,
            base_mint: None,
            error: Some("DRY RUN — no transaction sent".to_string()),
        });
    }

    tracing::info!(
        "close_position: position={} reason={}",
        &position_address[..8.min(position_address.len())],
        reason.unwrap_or("agent decision"),
    );

    // Execute close via Meridian JS CLI
    let mut cli_args = vec![
        "close",
        "--position", &position_address,
    ];

    if std::env::var("DRY_RUN").ok().as_deref() == Some("true") {
        cli_args.push("--dry-run");
    }

    match cli_exec(&cli_args).await {
        Ok(result) => {
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let pool = result.get("pool").and_then(|v| v.as_str()).map(String::from);
            let pool_name = result.get("pool_name").and_then(|v| v.as_str()).map(String::from);
            let pnl_usd = result.get("pnl_usd").and_then(|v| v.as_f64());
            let pnl_pct = result.get("pnl_pct").and_then(|v| v.as_f64());
            let base_mint = result.get("base_mint").and_then(|v| v.as_str()).map(String::from);
            let claim_txs = result.get("claim_txs").and_then(|v| v.as_str()).map(String::from);
            let close_txs = result.get("close_txs").and_then(|v| v.as_str()).map(String::from);
            let txs = result.get("txs").and_then(|v| v.as_str()).map(String::from);
            let error = if !success {
                result.get("error").and_then(|v| v.as_str()).map(String::from)
            } else {
                None
            };

            tracing::info!(
                "close_position result: success={} pnl_usd={:?} pnl_pct={:?}",
                success, pnl_usd, pnl_pct,
            );

            Ok(CloseResult {
                success,
                position: Some(position_address),
                pool,
                pool_name,
                claim_txs: claim_txs.map(|t| vec![t]),
                close_txs: close_txs.map(|t| vec![t]),
                txs: txs.map(|t| vec![t]),
                pnl_usd,
                pnl_pct,
                base_mint,
                error,
            })
        }
        Err(e) => {
            tracing::error!("close_position CLI failed: {}", e);
            Ok(CloseResult {
                success: false,
                position: Some(position_address),
                pool: None,
                pool_name: None,
                claim_txs: None,
                close_txs: None,
                txs: None,
                pnl_usd: None,
                pnl_pct: None,
                base_mint: None,
                error: Some(format!("CLI execution failed: {}", e)),
            })
        }
    }
}

/// Claim accumulated fees from a DLMM position.
///
/// Since the Meteora DLMM SDK crate is not available in Rust, this function
/// builds the claim transaction data. In the current implementation, it
/// returns a placeholder indicating what would be claimed.
pub async fn claim_fees(
    position_address: &str,
    _config: &Config,
) -> Result<ClaimResult> {
    let position_address = crate::tools::wallet::normalize_mint(position_address);

    if std::env::var("DRY_RUN").ok().as_deref() == Some("true") {
        return Ok(ClaimResult {
            success: false,
            position: Some(position_address),
            txs: None,
            base_mint: None,
            error: Some("DRY RUN — no transaction sent".to_string()),
        });
    }

    tracing::info!(
        "claim_fees: position={}",
        &position_address[..8.min(position_address.len())],
    );

    // Execute claim via Meridian JS CLI
    let cli_args = vec![
        "claim",
        "--position", &position_address,
    ];

    match cli_exec(&cli_args).await {
        Ok(result) => {
            let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
            let txs = result.get("txs").and_then(|v| v.as_str()).map(String::from);
            let base_mint = result.get("base_mint").and_then(|v| v.as_str()).map(String::from);
            let error = if !success {
                result.get("error").and_then(|v| v.as_str()).map(String::from)
            } else {
                None
            };

            tracing::info!(
                "claim_fees result: success={} base_mint={:?}",
                success, base_mint,
            );

            Ok(ClaimResult {
                success,
                position: Some(position_address),
                txs: txs.map(|t| vec![t]),
                base_mint,
                error,
            })
        }
        Err(e) => {
            tracing::error!("claim_fees CLI failed: {}", e);
            Ok(ClaimResult {
                success: false,
                position: Some(position_address),
                txs: None,
                base_mint: None,
                error: Some(format!("CLI execution failed: {}", e)),
            })
        }
    }
}

/// Search for DLMM pools by token symbol or address.
///
/// Calls the Meteora DLMM Data API pool search endpoint.
pub async fn search_pools(
    query: &str,
    limit: Option<usize>,
) -> Result<SearchPoolsResult> {
    let client = make_client();
    let limit = limit.unwrap_or(10);
    let url = format!(
        "{}/pools?query={}",
        METEORA_DLMM_API,
        urlencoding::encode(query),
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("Pool search API request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Pool search API error: {} {}", status, body));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse pool search response: {}", e))?;

    // Handle both array and object response formats
    let pools_raw = if let Some(arr) = data.as_array() {
        arr.clone()
    } else if let Some(arr) = data.get("data").and_then(|v| v.as_array()) {
        arr.clone()
    } else {
        vec![]
    };

    let pools: Vec<SearchPoolEntry> = pools_raw
        .into_iter()
        .take(limit)
        .map(|p| {
            SearchPoolEntry {
                pool: p
                    .get("address")
                    .or_else(|| p.get("pool_address"))
                    .and_then(|v| v.as_str())
                    .map(String::from),
                name: p.get("name").and_then(|v| v.as_str()).map(String::from),
                bin_step: p
                    .get("bin_step")
                    .or_else(|| {
                        p.get("dlmm_params")
                            .and_then(|dp| dp.get("bin_step"))
                    })
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u16),
                fee_pct: p
                    .get("base_fee_percentage")
                    .or_else(|| p.get("fee_pct"))
                    .and_then(|v| v.as_f64()),
                tvl: p.get("liquidity").and_then(|v| v.as_f64()),
                volume_24h: p.get("trade_volume_24h").and_then(|v| v.as_f64()),
                token_x: Some(SearchToken {
                    symbol: p
                        .get("mint_x_symbol")
                        .or_else(|| {
                            p.get("token_x")
                                .and_then(|tx| tx.get("symbol"))
                        })
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    mint: p
                        .get("mint_x")
                        .or_else(|| {
                            p.get("token_x")
                                .and_then(|tx| tx.get("address"))
                        })
                        .and_then(|v| v.as_str())
                        .map(String::from),
                }),
                token_y: Some(SearchToken {
                    symbol: p
                        .get("mint_y_symbol")
                        .or_else(|| {
                            p.get("token_y")
                                .and_then(|ty| ty.get("symbol"))
                        })
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    mint: p
                        .get("mint_y")
                        .or_else(|| {
                            p.get("token_y")
                                .and_then(|ty| ty.get("address"))
                        })
                        .and_then(|v| v.as_str())
                        .map(String::from),
                }),
            }
        })
        .collect();

    let total = pools.len();
    Ok(SearchPoolsResult {
        query: query.to_string(),
        total,
        pools,
    })
}

/// Get all positions for any wallet (including non-owned).
///
/// Uses the DLMM program accounts with memcmp filter to discover positions.
/// Since Solana RPC calls are complex in Rust without a full SDK, this
/// uses the Meteora PnL API as an alternative.
pub async fn get_wallet_positions(
    wallet_address: &str,
) -> Result<WalletPositionsResult> {
    let client = make_client();

    // Use portfolio API to discover positions
    let portfolio_url = format!(
        "{}/portfolio/open?user={}",
        METEORA_DLMM_API, wallet_address,
    );

    let portfolio_resp = client
        .get(&portfolio_url)
        .send()
        .await
        .map_err(|e| anyhow!("Wallet positions API request failed: {}", e))?;

    if !portfolio_resp.status().is_success() {
        let status = portfolio_resp.status();
        let body = portfolio_resp.text().await.unwrap_or_default();
        return Ok(WalletPositionsResult {
            wallet: wallet_address.to_string(),
            total_positions: 0,
            positions: vec![],
            error: Some(format!("Wallet positions API error: {} {}", status, body)),
        });
    }

    let portfolio: PortfolioResponse = portfolio_resp
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse wallet positions response: {}", e))?;

    let pools = portfolio.pools.unwrap_or_default();
    let mut all_positions = Vec::new();

    for pool in &pools {
        let pool_addr = pool.pool_address.as_deref().unwrap_or("");
        let position_addresses = pool.list_positions.as_ref()
            .map(|v| v.clone())
            .unwrap_or_default();

        // Fetch PnL for this pool
        let pnl_url = format!(
            "{}/positions/{}/pnl?user={}&status=open&pageSize=100&page=1",
            METEORA_DLMM_API, pool_addr, wallet_address,
        );

        let pnl_map: HashMap<String, PositionPnlData> = match client
            .get(&pnl_url)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                resp.json::<PnlPoolResponse>()
                    .await
                    .map(|data| {
                        let positions = data.positions.unwrap_or_default();
                        let mut map = HashMap::new();
                        for p in positions {
                            let addr = p.position_address
                                .as_deref()
                                .or(p.address.as_deref())
                                .or(p.position.as_deref())
                                .unwrap_or("")
                                .to_string();
                            if !addr.is_empty() {
                                map.insert(addr, p);
                            }
                        }
                        map
                    })
                    .unwrap_or_default()
            }
            _ => HashMap::new(),
        };

        for pos_addr in &position_addresses {
            let pnl = pnl_map.get(pos_addr);

            all_positions.push(PositionData {
                position: pos_addr.clone(),
                pool: pool_addr.to_string(),
                pair: None,
                base_mint: pool.token_x_mint.clone(),
                lower_bin: pnl.and_then(|p| p.lower_bin_id),
                upper_bin: pnl.and_then(|p| p.upper_bin_id),
                active_bin: pnl.and_then(|p| p.pool_active_bin_id),
                in_range: pnl.map(|p| !p.is_out_of_range.unwrap_or(false)),
                unclaimed_fees_usd: pnl.map(|p| {
                    let x = p.unrealized_pnl.as_ref()
                        .and_then(|u| u.unclaimed_fee_token_x.as_ref())
                        .and_then(|t| t.usd)
                        .unwrap_or(0.0);
                    let y = p.unrealized_pnl.as_ref()
                        .and_then(|u| u.unclaimed_fee_token_y.as_ref())
                        .and_then(|t| t.usd)
                        .unwrap_or(0.0);
                    round(x + y, 4)
                }),
                total_value_usd: pnl.map(|p| {
                    round(p.unrealized_pnl.as_ref().and_then(|u| u.balances).unwrap_or(0.0), 4)
                }),
                pnl_usd: pnl.map(|p| round(p.pnl_usd.unwrap_or(0.0), 4)),
                pnl_pct: pnl.and_then(|p| p.pnl_pct_change).map(|v| round(v, 2)),
                fee_per_tvl_24h: pnl.and_then(|p| p.fee_per_tvl_24h).map(|v| round(v, 2)),
                age_minutes: pnl.and_then(|p| {
                    p.created_at.map(|ts| {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs_f64();
                        ((now - ts) / 60.0) as i64
                    })
                }),
            });
        }
    }

    let total = all_positions.len();
    Ok(WalletPositionsResult {
        wallet: wallet_address.to_string(),
        total_positions: total,
        positions: all_positions,
        error: None,
    })
}

/// Fetch pool metadata from the Meteora DLMM Data API.
///
/// Returns pool name, token symbols, and other metadata.
async fn get_pool_metadata(pool_address: &str) -> Option<PoolMetadata> {
    let client = make_client();
    let url = format!("{}/pools/{}", METEORA_DLMM_API, pool_address);

    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    resp.json::<PoolMetadata>().await.ok()
}

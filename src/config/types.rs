use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub screening: ScreeningConfig,
    pub management: ManagementConfig,
    pub risk: RiskConfig,
    pub schedule: ScheduleConfig,
    pub llm: LlmConfig,
    pub strategy: StrategyConfig,
    #[serde(default)]
    pub dual_strategy: DualStrategyConfig,
    #[serde(default)]
    pub tokens: TokensConfig,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub jupiter: JupiterConfig,
    #[serde(default)]
    pub indicators: IndicatorsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreeningConfig {
    pub min_fee_active_tvl_ratio: f64,
    pub min_tvl: f64,
    #[serde(default)]
    pub max_tvl: Option<f64>,
    pub min_volume: f64,
    pub min_organic: f64,
    #[serde(default = "default_min_quote_organic")]
    pub min_quote_organic: f64,
    pub min_holders: u64,
    pub min_mcap: f64,
    pub max_mcap: f64,
    pub min_bin_step: u16,
    pub max_bin_step: u16,
    #[serde(default = "default_timeframe")]
    pub timeframe: String,
    #[serde(default = "default_category")]
    pub category: String,
    pub min_token_fees_sol: f64,
    #[serde(default = "default_max_bot_holders")]
    pub max_bot_holders_pct: f64,
    #[serde(default)]
    pub max_bundlers_pct: Option<f64>,
    #[serde(default = "default_max_top10")]
    pub max_top10_pct: f64,
    #[serde(default)]
    pub blocked_launchpads: Vec<String>,
    #[serde(default)]
    pub allowed_launchpads: Vec<String>,
    #[serde(default)]
    pub exclude_high_supply_concentration: bool,
    #[serde(default)]
    pub min_token_age_hours: Option<f64>,
    #[serde(default)]
    pub max_token_age_hours: Option<f64>,
    #[serde(default)]
    pub use_discord_signals: bool,
    #[serde(default)]
    pub discord_signal_mode: Option<String>,
}

fn default_min_quote_organic() -> f64 { 0.0 }
fn default_timeframe() -> String { "1h".to_string() }
fn default_category() -> String { "trending".to_string() }
fn default_max_bot_holders() -> f64 { 30.0 }
fn default_max_top10() -> f64 { 60.0 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagementConfig {
    pub deploy_amount_sol: f64,
    pub gas_reserve: f64,
    pub position_size_pct: f64,
    pub min_sol_to_open: f64,
    pub out_of_range_wait_minutes: u32,
    pub take_profit_pct: Option<f64>,
    #[serde(default = "default_management_interval")]
    pub management_interval_min: u32,
    #[serde(default = "default_screening_interval")]
    pub screening_interval_min: u32,
    // ── Trailing TP ──────────────────────────────────────────
    #[serde(default)]
    pub trailing_take_profit: bool,
    #[serde(default = "default_trailing_trigger_pct")]
    pub trailing_trigger_pct: f64,
    #[serde(default = "default_trailing_drop_pct")]
    pub trailing_drop_pct: f64,
    // ── Claim / Yield thresholds ─────────────────────────────
    #[serde(default = "default_min_claim_amount")]
    pub min_claim_amount: f64,
    #[serde(default = "default_min_fee_per_tvl_24h")]
    pub min_fee_per_tvl_24h: f64,
    #[serde(default = "default_min_age_before_yield_check")]
    pub min_age_before_yield_check: u32,
    // ── Pump / OOR bins ──────────────────────────────────────
    #[serde(default = "default_out_of_range_bins_to_close")]
    pub out_of_range_bins_to_close: i32,
    // ── Display mode ─────────────────────────────────────────
    #[serde(default)]
    pub sol_mode: bool,
}

fn default_management_interval() -> u32 { 10 }
fn default_screening_interval() -> u32 { 30 }
fn default_trailing_trigger_pct() -> f64 { 5.0 }
fn default_trailing_drop_pct() -> f64 { 3.0 }
fn default_min_claim_amount() -> f64 { 0.01 }
fn default_min_fee_per_tvl_24h() -> f64 { 0.0005 }
fn default_min_age_before_yield_check() -> u32 { 60 }
fn default_out_of_range_bins_to_close() -> i32 { 50 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskConfig {
    pub max_deploy_amount: f64,
    pub max_positions: u32,
    #[serde(default)]
    pub stop_loss_pct: Option<f64>,
    #[serde(default = "default_cooldown_loss")]
    pub cooldown_loss_pct: f64,
    #[serde(default = "default_cooldown_duration")]
    pub cooldown_duration_min: u32,
}

fn default_cooldown_loss() -> f64 { -5.0 }
fn default_cooldown_duration() -> u32 { 60 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleConfig {
    pub management_interval_min: u32,
    pub screening_interval_min: u32,
    #[serde(default = "default_pnl_poll_interval")]
    pub pnl_poll_interval_secs: u32,
    #[serde(default = "default_sync_interval")]
    pub sync_interval_min: u32,
}

fn default_pnl_poll_interval() -> u32 { 30 }
fn default_sync_interval() -> u32 { 5 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    pub management_model: String,
    pub screening_model: String,
    pub general_model: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
}

fn default_base_url() -> String { "https://openrouter.ai/api/v1".to_string() }
fn default_temperature() -> f32 { 0.7 }
fn default_max_tokens() -> u32 { 4096 }
fn default_max_steps() -> u32 { 20 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyConfig {
    pub min_bins_below: u32,
    pub max_bins_below: u32,
    #[serde(default = "default_min_safe_bins")]
    pub min_safe_bins_below: u32,
}

fn default_min_safe_bins() -> u32 { 35 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DualStrategyConfig {
    pub enabled: bool,
    pub primary_pct: f64,
    pub safeguard_oor_wait_min: u32,
    pub aggressive_oor_wait_min: u32,
}

impl Default for DualStrategyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            primary_pct: 0.6,
            safeguard_oor_wait_min: 60,
            aggressive_oor_wait_min: 15,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokensConfig {
    #[serde(default = "default_sol_mint")]
    pub sol_mint: String,
    #[serde(default = "default_usdc_mint")]
    pub usdc_mint: String,
}

fn default_sol_mint() -> String { "So11111111111111111111111111111111111111112".to_string() }
fn default_usdc_mint() -> String { "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiConfig {
    #[serde(default)]
    pub helius_api_key: Option<String>,
    #[serde(default)]
    pub helius_rpc_url: Option<String>,
    #[serde(default)]
    pub agent_meridian_base: Option<String>,
    #[serde(default)]
    pub agent_meridian_key: Option<String>,
    #[serde(default)]
    pub telegram_bot_token: Option<String>,
    #[serde(default)]
    pub telegram_chat_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JupiterConfig {
    #[serde(default)]
    pub referral_account: Option<String>,
    #[serde(default = "default_referral_fee_bps")]
    pub referral_fee_bps: u32,
    #[serde(default)]
    pub api_key: Option<String>,
}

fn default_referral_fee_bps() -> u32 { 50 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndicatorsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub presets: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            screening: ScreeningConfig {
                min_fee_active_tvl_ratio: 0.05,
                min_tvl: 10_000.0,
                max_tvl: None,
                min_volume: 500.0,
                min_organic: 60.0,
                min_quote_organic: 0.0,
                min_holders: 500,
                min_mcap: 150_000.0,
                max_mcap: 10_000_000.0,
                min_bin_step: 80,
                max_bin_step: 125,
                timeframe: "1h".to_string(),
                category: "trending".to_string(),
                min_token_fees_sol: 30.0,
                max_bot_holders_pct: 30.0,
                max_bundlers_pct: None,
                max_top10_pct: 60.0,
                blocked_launchpads: vec![],
                allowed_launchpads: vec![],
                exclude_high_supply_concentration: false,
                min_token_age_hours: None,
                max_token_age_hours: None,
                use_discord_signals: false,
                discord_signal_mode: None,
            },
            management: ManagementConfig {
                deploy_amount_sol: 0.5,
                gas_reserve: 0.2,
                position_size_pct: 0.35,
                min_sol_to_open: 0.55,
                out_of_range_wait_minutes: 30,
                take_profit_pct: None,
                management_interval_min: 10,
                screening_interval_min: 30,
                trailing_take_profit: false,
                trailing_trigger_pct: 5.0,
                trailing_drop_pct: 3.0,
                min_claim_amount: 0.01,
                min_fee_per_tvl_24h: 0.0005,
                min_age_before_yield_check: 60,
                out_of_range_bins_to_close: 50,
                sol_mode: false,
            },
            risk: RiskConfig {
                max_deploy_amount: 50.0,
                max_positions: 3,
                stop_loss_pct: None,
                cooldown_loss_pct: -5.0,
                cooldown_duration_min: 60,
            },
            schedule: ScheduleConfig {
                management_interval_min: 10,
                screening_interval_min: 30,
                pnl_poll_interval_secs: 30,
                sync_interval_min: 5,
            },
            llm: LlmConfig {
                management_model: "openrouter/healer-alpha".to_string(),
                screening_model: "openrouter/healer-alpha".to_string(),
                general_model: "openrouter/healer-alpha".to_string(),
                base_url: "https://openrouter.ai/api/v1".to_string(),
                api_key: None,
                temperature: 0.7,
                max_tokens: 4096,
                max_steps: 20,
            },
            strategy: StrategyConfig {
                min_bins_below: 15,
                max_bins_below: 50,
                min_safe_bins_below: 35,
            },
            dual_strategy: DualStrategyConfig::default(),
            tokens: TokensConfig::default(),
            api: ApiConfig::default(),
            jupiter: JupiterConfig::default(),
            indicators: IndicatorsConfig::default(),
        }
    }
}

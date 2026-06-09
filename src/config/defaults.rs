use super::types::*;

/// Hardcoded VPS config — no user-config.json needed.
/// Sensitive values (RPC, API keys, wallet) still come from env vars.
pub fn vps_config() -> Config {
    Config {
        dry_run: true,

        screening: ScreeningConfig {
            min_fee_active_tvl_ratio: 0.01,
            min_tvl: 3_000.0,
            max_tvl: Some(500_000.0),
            min_volume: 200.0,
            min_organic: 40.0,
            min_quote_organic: 40.0,
            min_holders: 300,
            min_mcap: 50_000.0,
            max_mcap: 50_000_000.0,
            min_bin_step: 80,
            max_bin_step: 125,
            timeframe: "1h".to_string(),
            category: "trending".to_string(),
            min_token_fees_sol: 30.0,
            max_bot_holders_pct: 30.0,
            max_bundlers_pct: Some(30.0),
            max_top10_pct: 60.0,
            blocked_launchpads: vec![],
            allowed_launchpads: vec![],
            exclude_high_supply_concentration: true,
            min_token_age_hours: None,
            max_token_age_hours: None,
            use_discord_signals: false,
            discord_signal_mode: Some("merge".to_string()),
            avoid_pvp_symbols: true,
            block_pvp_symbols: false,
        },

        management: ManagementConfig {
            deploy_amount_sol: 0.5,
            gas_reserve: 0.2,
            position_size_pct: 0.35,
            min_sol_to_open: 0.55,
            out_of_range_wait_minutes: 30,
            oor_cooldown_trigger_count: 3,
            oor_cooldown_hours: 12,
            repeat_deploy_cooldown_enabled: true,
            repeat_deploy_cooldown_trigger_count: 3,
            repeat_deploy_cooldown_hours: 12,
            repeat_deploy_cooldown_scope: "token".to_string(),
            repeat_deploy_cooldown_min_fee_earned_pct: 0.0,
            take_profit_pct: Some(5.0),
            management_interval_min: 10,
            screening_interval_min: 30,
            trailing_take_profit: true,
            trailing_trigger_pct: 3.0,
            trailing_drop_pct: 1.5,
            min_claim_amount: 5.0,
            min_fee_per_tvl_24h: 7.0,
            min_age_before_yield_check: 60,
            out_of_range_bins_to_close: 10,
            sol_mode: false,
        },

        risk: RiskConfig {
            max_deploy_amount: 50.0,
            max_positions: 3,
            stop_loss_pct: Some(-50.0),
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
            management_model: std::env::var("MANAGEMENT_MODEL")
                .unwrap_or_else(|_| "mimo-v2.5".to_string()),
            screening_model: std::env::var("SCREENING_MODEL")
                .unwrap_or_else(|_| "mimo-v2.5".to_string()),
            general_model: std::env::var("GENERAL_MODEL")
                .unwrap_or_else(|_| "mimo-v2.5".to_string()),
            base_url: std::env::var("LLM_BASE_URL")
                .unwrap_or_else(|_| "https://token-plan-sgp.xiaomimimo.com/v1".to_string()),
            api_key: std::env::var("LLM_API_KEY")
                .ok()
                .or_else(|| std::env::var("OPENROUTER_API_KEY").ok()),
            temperature: 0.373,
            max_tokens: 4096,
            max_steps: 20,
        },

        strategy: StrategyConfig {
            min_bins_below: 35,
            max_bins_below: 69,
            min_safe_bins_below: 35,
        },

        dual_strategy: DualStrategyConfig {
            enabled: false,
            primary_pct: 0.6,
            safeguard_oor_wait_min: 60,
            aggressive_oor_wait_min: 15,
        },

        tokens: TokensConfig::default(),

        api: ApiConfig {
            helius_rpc_url: std::env::var("RPC_URL")
                .ok()
                .or_else(|| std::env::var("HELIUS_RPC_URL").ok()),
            helius_api_key: std::env::var("HELIUS_API_KEY").ok(),
            agent_meridian_base: std::env::var("AGENT_MERIDIAN_API_URL")
                .ok()
                .map(Some)
                .unwrap_or_else(|| Some("https://api.agentmeridian.xyz/api".to_string())),
            agent_meridian_key: std::env::var("PUBLIC_API_KEY").ok(),
            lp_agent_relay_enabled: true,
            telegram_bot_token: std::env::var("TELEGRAM_BOT_TOKEN").ok(),
            telegram_chat_id: std::env::var("TELEGRAM_CHAT_ID").ok(),
        },

        jupiter: JupiterConfig {
            api_key: std::env::var("JUPITER_API_KEY").ok(),
            referral_account: std::env::var("JUPITER_REFERRAL_ACCOUNT").ok(),
            referral_fee_bps: std::env::var("JUPITER_REFERRAL_FEE_BPS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(25),
        },

        indicators: IndicatorsConfig {
            enabled: false,
            entry_preset: Some("supertrend_break".to_string()),
            exit_preset: Some("supertrend_break".to_string()),
            rsi_length: 2,
            intervals: vec!["5_MINUTE".to_string()],
            candles: 298,
            rsi_oversold: 30.0,
            rsi_overbought: 80.0,
            require_all_intervals: false,
            presets: vec!["supertrend_break".to_string()],
        },

        darwin: DarwinConfig {
            enabled: true,
            window_days: 60,
            recalc_every: 5,
            boost_factor: 1.05,
            decay_factor: 0.95,
            weight_floor: 0.3,
            weight_ceiling: 2.5,
            min_samples: 10,
        },
    }
}

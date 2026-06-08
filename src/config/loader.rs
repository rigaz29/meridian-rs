use super::types::Config;
use anyhow::Result;
use std::path::PathBuf;

fn find_config_path(explicit: Option<&str>) -> PathBuf {
    if let Some(p) = explicit {
        return PathBuf::from(p);
    }
    // ~/.meridian/user-config.json > ./user-config.json
    let home_config = dirs_home().join("user-config.json");
    if home_config.exists() { return home_config; }
    PathBuf::from("user-config.json")
}

fn dirs_home() -> PathBuf {
    if let Ok(h) = std::env::var("MERIDIAN_HOME") {
        return PathBuf::from(h);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".meridian")
}

pub fn load_config(path: Option<&str>) -> Result<Config> {
    let config_path = find_config_path(path);
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        let config: Config = serde_json::from_str(&content)?;
        tracing::info!("Loaded config from {}", config_path.display());
        Ok(config)
    } else {
        tracing::warn!("No config file found at {}, using defaults", config_path.display());
        Ok(Config::default())
    }
}

pub fn save_config(config: &Config, path: Option<&str>) -> Result<()> {
    let config_path = path.map(PathBuf::from).unwrap_or_else(|| find_config_path(None));
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&config_path, content)?;
    Ok(())
}

/// Compute deploy amount based on wallet SOL balance
pub fn compute_deploy_amount(config: &Config, wallet_sol: f64) -> f64 {
    let by_pct = wallet_sol * config.management.position_size_pct;
    let amount = by_pct.min(config.risk.max_deploy_amount).max(config.management.deploy_amount_sol);
    if wallet_sol - amount < config.management.gas_reserve {
        return 0.0; // not enough SOL after gas reserve
    }
    amount
}

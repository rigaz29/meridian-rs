use std::path::PathBuf;

pub(super) fn dirs_home() -> PathBuf {
    if let Ok(h) = std::env::var("MERIDIAN_HOME") {
        return PathBuf::from(h);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".meridian")
}

pub fn load_env_files() {
    // Match the original JS project convention: allow repo-local `.env` for app
    // runs and `~/.meridian/.env` for globally installed CLI/runtime usage.
    let home_env = dirs_home().join(".env");
    if home_env.exists() {
        let _ = dotenvy::from_path(&home_env);
    }
    let repo_env = PathBuf::from(".env");
    if repo_env.exists() {
        let _ = dotenvy::from_path(&repo_env);
    }
}

pub fn meridian_data_path(file_name: &str) -> PathBuf {
    let dir = std::env::var("MERIDIAN_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs_home());
    let _ = std::fs::create_dir_all(&dir);
    dir.join(file_name)
}

/// Compute deploy amount based on wallet SOL balance
pub fn compute_deploy_amount(config: &crate::config::Config, wallet_sol: f64) -> f64 {
    let by_pct = wallet_sol * config.management.position_size_pct;
    let amount = by_pct
        .min(config.risk.max_deploy_amount)
        .max(config.management.deploy_amount_sol);
    if wallet_sol - amount < config.management.gas_reserve {
        return 0.0; // not enough SOL after gas reserve
    }
    amount
}

#[cfg(test)]
mod tests {
    use super::meridian_data_path;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const ENV_KEYS: &[&str] = &[
        "DRY_RUN",
        "MERIDIAN_DATA_DIR",
        "RPC_URL",
        "HELIUS_RPC_URL",
        "HELIUS_API_KEY",
        "LLM_BASE_URL",
        "LLM_API_KEY",
        "OPENROUTER_API_KEY",
        "LLM_MODEL",
        "MANAGEMENT_MODEL",
        "SCREENING_MODEL",
        "GENERAL_MODEL",
        "TELEGRAM_BOT_TOKEN",
        "TELEGRAM_CHAT_ID",
        "AGENT_MERIDIAN_API_URL",
        "PUBLIC_API_KEY",
        "LPAGENT_API_KEY",
        "JUPITER_API_KEY",
        "JUPITER_REFERRAL_ACCOUNT",
        "JUPITER_REFERRAL_FEE_BPS",
    ];

    struct EnvGuard {
        saved: Vec<(&'static str, Option<String>)>,
    }

    impl EnvGuard {
        fn clear(keys: &'static [&'static str]) -> Self {
            let saved = keys
                .iter()
                .map(|key| (*key, std::env::var(key).ok()))
                .collect::<Vec<_>>();
            for key in keys {
                std::env::remove_var(key);
            }
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.saved {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    #[test]
    fn meridian_data_path_uses_isolated_data_dir() {
        let _lock = ENV_LOCK.lock().expect("env test lock");
        let _env = EnvGuard::clear(ENV_KEYS);
        let data_dir = std::env::temp_dir().join(format!(
            "meridian-rs-data-dir-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::env::set_var("MERIDIAN_DATA_DIR", &data_dir);

        let path = meridian_data_path("meridian-state.json");

        assert_eq!(path, data_dir.join("meridian-state.json"));
        assert!(data_dir.exists());
        let _ = std::fs::remove_dir_all(&data_dir);
    }
}

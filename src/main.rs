#![allow(dead_code)]

use anyhow::Result;

mod config;
mod tools;
mod utils;
mod state;
mod agent;
mod web;
mod llm;
mod cycle;
mod lessons;
mod models;

use config::load_config;
use config::llm_config::LlmCredentials;
use state::positions::PositionState;
use state::pool_memory::PoolMemoryStore;
use llm::LlmClient;
use utils::logger::module::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .init();

    info("main", "Meridian RS -- DLMM Liquidity Provider Agent v0.2.0");
    info("main", "================================================");

    let config = load_config(None)?;
    info("main", "Config loaded");

    let creds = LlmCredentials::from_env_or_config(
        Some(&config.llm.base_url),
        config.llm.api_key.as_deref(),
    );
    let _llm = LlmClient::new(&creds.api_key, &creds.base_url);
    info("main", &format!("LLM client ready -- {}", &config.llm.base_url));

    let state_path = std::env::var("MERIDIAN_STATE_PATH")
        .unwrap_or_else(|_| "meridian-state.json".to_string());
    let positions = PositionState::load(&state_path)?;
    let _pool_memory = PoolMemoryStore::load("pool-memory.json")?;
    info("main", &format!("State loaded -- {} active positions", positions.count_active()));

    let _wallet_balance = 0.0f64;

    info("main", "Starting cycle scheduler...");

    let mgmt_interval = tokio::time::Duration::from_secs(
        config.schedule.management_interval_min as u64 * 60
    );
    let screen_interval = tokio::time::Duration::from_secs(
        config.schedule.screening_interval_min as u64 * 60
    );

    // Clone for spawned tasks
    let config_mgmt = config.clone();
    let llm_mgmt = LlmClient::new(&creds.api_key, &creds.base_url);
    let mgmt_state_path = state_path.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(mgmt_interval);
        interval.tick().await;
        loop {
            interval.tick().await;
            let positions = match PositionState::load(&mgmt_state_path) {
                Ok(p) => p,
                Err(e) => { warn("mgmt", &format!("Failed to load positions: {}", e)); continue; }
            };
            let pool_memory = match PoolMemoryStore::load("pool-memory.json") {
                Ok(p) => p,
                Err(e) => { warn("mgmt", &format!("Failed to load pool memory: {}", e)); continue; }
            };
            match cycle::run_management_cycle(&config_mgmt, &llm_mgmt, &positions, &pool_memory).await {
                Ok(result) => info("mgmt", &format!("Management cycle done: {}", &result[..result.len().min(200)])),
                Err(e) => warn("mgmt", &format!("Management cycle error: {}", e)),
            }
        }
    });

    let config_screen = config.clone();
    let llm_screen = LlmClient::new(&creds.api_key, &creds.base_url);
    let screen_state_path = state_path.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(screen_interval);
        interval.tick().await;
        loop {
            interval.tick().await;
            let positions = match PositionState::load(&screen_state_path) {
                Ok(p) => p,
                Err(e) => { warn("screen", &format!("Failed to load positions: {}", e)); continue; }
            };
            let pool_memory = match PoolMemoryStore::load("pool-memory.json") {
                Ok(p) => p,
                Err(e) => { warn("screen", &format!("Failed to load pool memory: {}", e)); continue; }
            };
            match cycle::run_screening_cycle(&config_screen, &llm_screen, &positions, &pool_memory, _wallet_balance).await {
                Ok(result) => info("screen", &format!("Screening cycle done: {}", &result[..result.len().min(200)])),
                Err(e) => warn("screen", &format!("Screening cycle error: {}", e)),
            }
        }
    });

    info("main", "Cycles scheduled. Starting web server on :3000...");
    info("main", &format!("Management interval: {} min", config.schedule.management_interval_min));
    info("main", &format!("Screening interval: {} min", config.schedule.screening_interval_min));

    web::start_web_server().await?;

    Ok(())
}

#![allow(dead_code)]

use anyhow::Result;
use chrono::Timelike;
use tokio::signal;
use tokio::sync::watch;

mod agent;
mod config;
mod cycle;
mod lessons;
mod llm;
mod models;
mod state;
mod tools;
mod utils;
mod web;

use config::llm_config::LlmCredentials;
use config::load_config;
use cycle::{run_management_cycle, run_pnl_poll, run_screening_cycle};
use llm::LlmClient;
use state::pool_memory::PoolMemoryStore;
use state::positions::PositionState;
use utils::logger::module::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .init();

    info(
        "main",
        "Meridian RS -- DLMM Liquidity Provider Agent v0.2.0",
    );
    info("main", "================================================");

    let config = load_config(None)?;
    info("main", "Config loaded");

    let creds = LlmCredentials::from_env_or_config(
        Some(&config.llm.base_url),
        config.llm.api_key.as_deref(),
    );
    let llm = LlmClient::new(&creds.api_key, &creds.base_url);
    info(
        "main",
        &format!("LLM client ready -- {}", &config.llm.base_url),
    );

    let state_path =
        std::env::var("MERIDIAN_STATE_PATH").unwrap_or_else(|_| "meridian-state.json".to_string());
    let mut positions = PositionState::load(&state_path)?;
    let pool_memory = PoolMemoryStore::load("pool-memory.json")?;
    info(
        "main",
        &format!(
            "State loaded -- {} active positions",
            positions.count_active()
        ),
    );

    // Read wallet address from env or config
    let wallet_address = std::env::var("MERIDIAN_WALLET")
        .unwrap_or_else(|_| "".to_string());

    // ── Graceful shutdown channel ──────────────────────────────
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Spawn Ctrl+C handler
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        info("shutdown", "Ctrl+C received, shutting down gracefully...");
        let _ = shutdown_tx_clone.send(true);
    });

    info("main", "Starting cycle scheduler...");

    // ── PnL Poller (every 30s, lightweight, no LLM) ───────────
    let pnl_interval = tokio::time::Duration::from_secs(
        config.schedule.pnl_poll_interval_secs as u64,
    );
    let config_pnl = config.clone();
    let state_path_pnl = state_path.clone();
    let wallet_pnl = wallet_address.clone();
    let mut shutdown_pnl = shutdown_rx.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(pnl_interval);
        interval.tick().await; // skip first tick
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = shutdown_pnl.changed() => {
                    info("pnl_poll", "Shutdown signal received, stopping PnL poller");
                    break;
                }
            }

            let mut positions = match PositionState::load(&state_path_pnl) {
                Ok(p) => p,
                Err(e) => {
                    warn("pnl_poll", &format!("Failed to load positions: {}", e));
                    continue;
                }
            };

            match run_pnl_poll(&config_pnl, &mut positions, &wallet_pnl).await {
                Ok(exits) => {
                    if !exits.is_empty() {
                        // Save state after exit detection
                        if let Err(e) = positions.save(&state_path_pnl) {
                            warn("pnl_poll", &format!("Failed to save state: {}", e));
                        }
                        // TODO: trigger close actions for exits
                        for (addr, reason) in &exits {
                            info("pnl_poll", &format!("Exit needed: {} — {}", addr, reason));
                        }
                        // Set instruction on positions needing close so management cycle picks them up
                        for (addr, reason) in &exits {
                            positions.set_instruction(addr, Some(&format!("CLOSE: {}", reason)));
                        }
                        // Save again after setting instructions
                        if let Err(e) = positions.save(&state_path_pnl) {
                            warn("pnl_poll", &format!("Failed to save state after instructions: {}", e));
                        }
                    }
                }
                Err(e) => {
                    warn("pnl_poll", &format!("PnL poll error: {}", e));
                }
            }
        }
    });

    // ── Management Cycle (every N minutes) ────────────────────
    let mgmt_interval =
        tokio::time::Duration::from_secs(config.schedule.management_interval_min as u64 * 60);
    let config_mgmt = config.clone();
    let llm_mgmt = LlmClient::new(&creds.api_key, &creds.base_url);
    let mgmt_state_path = state_path.clone();
    let wallet_mgmt = wallet_address.clone();
    let mut shutdown_mgmt = shutdown_rx.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(mgmt_interval);
        interval.tick().await;
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = shutdown_mgmt.changed() => {
                    info("mgmt", "Shutdown signal received, stopping management cycle");
                    break;
                }
            }

            let mut positions = match PositionState::load(&mgmt_state_path) {
                Ok(p) => p,
                Err(e) => {
                    warn("mgmt", &format!("Failed to load positions: {}", e));
                    continue;
                }
            };
            let mut pool_memory = match PoolMemoryStore::load("pool-memory.json") {
                Ok(pm) => pm,
                Err(e) => {
                    warn("mgmt", &format!("Failed to load pool memory: {}", e));
                    continue;
                }
            };

            match run_management_cycle(&config_mgmt, &llm_mgmt, &mut positions, &mut pool_memory, &wallet_mgmt).await {
                Ok(result) => {
                    info("mgmt", &format!("Management cycle complete: {}", &result[..result.len().min(200)]));
                    if let Err(e) = positions.save(&mgmt_state_path) {
                        warn("mgmt", &format!("Failed to save state: {}", e));
                    }
                }
                Err(e) => {
                    warn("mgmt", &format!("Management cycle error: {}", e));
                }
            }
        }
    });

    // ── Screening Cycle (every N minutes) ─────────────────────
    let screen_interval =
        tokio::time::Duration::from_secs(config.schedule.screening_interval_min as u64 * 60);
    let config_screen = config.clone();
    let llm_screen = LlmClient::new(&creds.api_key, &creds.base_url);
    let screen_state_path = state_path.clone();
    let wallet_screen = wallet_address.clone();
    let mut shutdown_screen = shutdown_rx.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(screen_interval);
        interval.tick().await;
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = shutdown_screen.changed() => {
                    info("screen", "Shutdown signal received, stopping screening cycle");
                    break;
                }
            }

            let mut positions = match PositionState::load(&screen_state_path) {
                Ok(p) => p,
                Err(e) => {
                    warn("screen", &format!("Failed to load positions: {}", e));
                    continue;
                }
            };
            let mut pool_memory = match PoolMemoryStore::load("pool-memory.json") {
                Ok(pm) => pm,
                Err(e) => {
                    warn("screen", &format!("Failed to load pool memory: {}", e));
                    continue;
                }
            };

            // Fetch real wallet SOL balance
            let wallet_sol = {
                let rpc = config_screen.api.helius_rpc_url.as_deref().unwrap_or("https://api.mainnet-beta.solana.com");
                let helius_key = config_screen.api.helius_api_key.as_deref().unwrap_or("");
                match crate::tools::wallet::get_wallet_balances(rpc, &wallet_screen, helius_key).await {
                    Ok(balances) => balances.sol,
                    Err(e) => {
                        warn("screen", &format!("Failed to fetch wallet balance: {}", e));
                        0.0
                    }
                }
            };

            match run_screening_cycle(&config_screen, &llm_screen, &mut positions, &mut pool_memory, wallet_sol, &wallet_screen).await {
                Ok(result) => {
                    info("screen", &format!("Screening cycle complete: {}", &result[..result.len().min(200)]));
                }
                Err(e) => {
                    warn("screen", &format!("Screening cycle error: {}", e));
                }
            }

            // Save state after screening (in case deploy happened)
            if let Err(e) = positions.save(&screen_state_path) {
                warn("screen", &format!("Failed to save state: {}", e));
            }
        }
    });


    // ── Briefing Cycle (daily at 01:00 UTC) ─────────────────────
    let config_brief = config.clone();
    let state_brief = state_path.clone();
    let wallet_brief = wallet_address.clone();
    let mut shutdown_brief = shutdown_rx.clone();

    tokio::spawn(async move {
        // Run briefing once at startup if it's between 01:00-02:00 UTC
        let now = chrono::Utc::now();
        let hour = now.hour();
        let minute = now.minute();
        let mut ran_today = hour == 1 && minute < 30;

        let mut interval = tokio::time::Duration::from_secs(3600); // check every hour
        let mut tick = tokio::time::interval(interval);
        tick.tick().await;

        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = shutdown_brief.changed() => {
                    info("briefing", "Shutdown signal received");
                    break;
                }
            }

            let now = chrono::Utc::now();
            if now.hour() == 1 && !ran_today {
                ran_today = true;

                // Build briefing
                let positions = match PositionState::load(&state_brief) {
                    Ok(p) => p,
                    Err(e) => {
                        warn("briefing", &format!("Failed to load positions: {}", e));
                        continue;
                    }
                };

                let state_summary = positions.get_state_summary();
                let active = positions.get_active();
                let active_count = active.len();

                let mut briefing = format!(
                    "📊 *Meridian Daily Briefing*\n{}\n\n*Open Positions:* {}\n\n{}",
                    now.format("%Y-%m-%d %H:%M UTC"),
                    active_count,
                    state_summary,
                );

                // Send via Telegram if configured
                if let (Some(bot_token), Some(chat_id)) = (
                    config_brief.api.telegram_bot_token.as_deref(),
                    config_brief.api.telegram_chat_id.as_deref(),
                ) {
                    if let Err(e) = crate::tools::telegram::send_message_safe(bot_token, chat_id, &briefing).await {
                        warn("briefing", &format!("Telegram send failed: {}", e));
                    } else {
                        info("briefing", "Daily briefing sent to Telegram");
                    }
                }

                // Reset ran_today at midnight UTC
                if now.hour() == 23 {
                    ran_today = false;
                }
            }
        }
    });

    // ── Health check endpoint (TCP listener) ───────────────────
    let health_port: u16 = std::env::var("HEALTH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let mut shutdown_health = shutdown_rx.clone();

    tokio::spawn(async move {
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], health_port));
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                warn("health", &format!("Health bind failed: {}", e));
                return;
            }
        };
        info("health", &format!("Health check on :{}", health_port));

        loop {
            tokio::select! {
                accept = listener.accept() => {
                    if let Ok((mut stream, _)) = accept {
                        let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"status\":\"ok\",\"version\":\"0.2.0\"}\r\n";
                        use tokio::io::AsyncWriteExt;
                        let _ = stream.write_all(resp.as_bytes()).await;
                    }
                }
                _ = shutdown_health.changed() => {
                    info("health", "Health check shutting down");
                    break;
                }
            }
        }
    });

    // ── REPL (interactive mode) ────────────────────────────────
    let is_tty = atty::is(atty::Stream::Stdin);
    if is_tty {
        info("main", "Interactive mode — type 'help' for commands, 'quit' to exit");
        let mut shutdown_repl = shutdown_rx.clone();

        loop {
            tokio::select! {
                _ = shutdown_repl.changed() => {
                    info("main", "Shutdown signal received");
                    break;
                }
                line = tokio::task::spawn_blocking(|| {
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).ok();
                    input.trim().to_string()
                }) => {
                    let line = line.unwrap_or_default();
                    match line.as_str() {
                        "quit" | "exit" | "q" => {
                            info("main", "Exiting...");
                            let _ = shutdown_tx.send(true);
                            break;
                        }
                        "help" | "h" => {
                            println!("Commands:");
                            println!("  status    — Show position state summary");
                            println!("  screen    — Run screening cycle now");
                            println!("  manage    — Run management cycle now");
                            println!("  quit/exit — Graceful shutdown");
                        }
                        "status" | "s" => {
                            let positions = PositionState::load(&state_path).unwrap_or_default();
                            println!("{}", positions.get_state_summary());
                        }
                        "screen" => {
                            info("main", "Manual screening triggered");
                            // Would run screening cycle
                        }
                        "manage" | "m" => {
                            info("main", "Manual management triggered");
                            // Would run management cycle
                        }
                        "" => {}
                        _ => {
                            println!("Unknown command: {}. Type 'help' for commands.", line);
                        }
                    }
                }
            }
        }
    } else {
        // Non-interactive: wait for shutdown signal
        info("main", "Non-interactive mode, waiting for shutdown signal...");
        let _ = shutdown_rx.clone().changed().await;
    }

    // ── Cleanup ────────────────────────────────────────────────
    info("main", "Saving final state...");
    if let Err(e) = positions.save(&state_path) {
        warn("main", &format!("Failed to save final state: {}", e));
    }
    info("main", "Meridian RS shutdown complete. Goodbye! 🧙");

    Ok(())
}

//! Interactive Telegram control for the Meridian bot.
//!
//! Long-polls `getUpdates`, authorizes the single admin chat, and dispatches
//! commands to the existing CLI command surface (`parse_cli_args` +
//! `run_cli_command`) so there is one source of truth for bot actions. The
//! `/start` and `/stop` commands flip a shared `trading_enabled` flag that the
//! screening cycle checks before deploying — pausing NEW deploys while still
//! managing/closing open positions. Admin-only; everyone else is rejected.

use crate::cli::{parse_cli_args, run_cli_command, CliOutput};
use crate::config::types::Config;
use crate::utils::logger::module::{info, warn};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const TG_API: &str = "https://api.telegram.org";
const MAX_TG_LEN: usize = 3800; // Telegram caps messages at 4096 chars

const HELP: &str = "🤖 *Meridian control*\n\
/status — agent state + open positions\n\
/positions — open positions detail\n\
/pnl — portfolio PnL (realized + unrealized)\n\
/balance — wallet SOL balance\n\
/candidates [n] — top screening candidates\n\
/start — resume trading (new deploys)\n\
/stop — pause new deploys (still manages open)\n\
/close <pool|position> — close a position\n\
/help — this message";

/// Spawned from `main`. Never returns; loops on getUpdates.
pub async fn run(config: Config, state_path: String, trading_enabled: Arc<AtomicBool>) {
    let token = match config
        .api
        .telegram_bot_token
        .clone()
        .filter(|s| !s.is_empty())
    {
        Some(t) => t,
        None => {
            info(
                "telegram",
                "interactive control disabled (no telegram_bot_token)",
            );
            return;
        }
    };
    let admin = match config.api.telegram_chat_id.clone().filter(|s| !s.is_empty()) {
        Some(c) => c,
        None => {
            info(
                "telegram",
                "interactive control disabled (no telegram_chat_id)",
            );
            return;
        }
    };

    let client = reqwest::Client::new();
    let _ =
        crate::tools::telegram::send_message_safe(&token, &admin, "🤖 Meridian control online — /help")
            .await;
    info("telegram", "interactive control online");

    let mut offset: i64 = 0;
    loop {
        match get_updates(&client, &token, offset).await {
            Ok(updates) => {
                for upd in updates {
                    let id = upd.get("update_id").and_then(Value::as_i64).unwrap_or(offset);
                    offset = id + 1;

                    let Some(msg) = upd.get("message") else {
                        continue;
                    };
                    let from_chat = msg
                        .get("chat")
                        .and_then(|c| c.get("id"))
                        .and_then(Value::as_i64)
                        .map(|i| i.to_string())
                        .unwrap_or_default();
                    let text = msg.get("text").and_then(Value::as_str).unwrap_or("");
                    if text.is_empty() {
                        continue;
                    }

                    if from_chat != admin {
                        warn("telegram", &format!("rejected non-admin chat {from_chat}"));
                        let _ = crate::tools::telegram::send_message_safe(
                            &token,
                            &from_chat,
                            "⛔ Unauthorized.",
                        )
                        .await;
                        continue;
                    }

                    let reply = handle(text, &config, &state_path, &trading_enabled).await;
                    let _ =
                        crate::tools::telegram::send_message_safe(&token, &admin, &reply).await;
                }
            }
            Err(e) => {
                warn("telegram", &format!("getUpdates error: {e}"));
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn get_updates(
    client: &reqwest::Client,
    token: &str,
    offset: i64,
) -> anyhow::Result<Vec<Value>> {
    // Long-poll (30s) so we react promptly without hammering the API.
    let url = format!("{TG_API}/bot{token}/getUpdates?timeout=30&offset={offset}");
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(40))
        .send()
        .await?;
    let body: Value = resp.json().await?;
    Ok(body
        .get("result")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

async fn handle(
    text: &str,
    config: &Config,
    state_path: &str,
    trading_enabled: &Arc<AtomicBool>,
) -> String {
    let mut it = text.trim().split_whitespace();
    let raw = it.next().unwrap_or("");
    // strip leading '/' and any '@botname' suffix
    let cmd = raw
        .trim_start_matches('/')
        .split('@')
        .next()
        .unwrap_or("")
        .to_lowercase();
    let rest: Vec<String> = it.map(|s| s.to_string()).collect();

    match cmd.as_str() {
        "" | "help" => HELP.to_string(),
        "start" => {
            trading_enabled.store(true, Ordering::SeqCst);
            "▶️ Trading ENABLED — bot will deploy on valid candidates.".to_string()
        }
        "stop" => {
            trading_enabled.store(false, Ordering::SeqCst);
            "⏸️ Trading PAUSED — no new deploys. Open positions still managed & closed.".to_string()
        }
        "pnl" => portfolio_text(config).await,
        "status" => {
            let flag = if trading_enabled.load(Ordering::SeqCst) {
                "▶️ trading ENABLED"
            } else {
                "⏸️ trading PAUSED"
            };
            let base = run_cli("status", &[], config, state_path).await;
            format!("{flag}\n{base}")
        }
        "positions" => run_cli("positions", &[], config, state_path).await,
        "balance" => run_cli("balance", &[], config, state_path).await,
        "candidates" => {
            let lim = rest.first().cloned().unwrap_or_else(|| "8".to_string());
            run_cli("candidates", &["--limit".to_string(), lim], config, state_path).await
        }
        "close" => match rest.first() {
            Some(target) => {
                run_cli(
                    "close",
                    &["--position".to_string(), target.clone()],
                    config,
                    state_path,
                )
                .await
            }
            None => "Usage: /close <pool_or_position_address>".to_string(),
        },
        other => format!("Unknown command: /{other}\n\n{HELP}"),
    }
}

/// Run a CLI command by reusing the argv parser (args[0] is a placeholder).
async fn run_cli(cmd: &str, tail: &[String], config: &Config, state_path: &str) -> String {
    let mut args = vec!["meridian".to_string(), cmd.to_string()];
    args.extend_from_slice(tail);
    match parse_cli_args(&args) {
        Ok(Some(command)) => match run_cli_command(command, config, state_path).await {
            Ok(out) => render(out),
            Err(e) => format!("⚠️ {cmd} failed: {e}"),
        },
        Ok(None) => format!("⚠️ could not parse /{cmd}"),
        Err(e) => format!("⚠️ parse error: {e}"),
    }
}

fn render(out: CliOutput) -> String {
    let s = match out {
        CliOutput::Text(t) => t,
        CliOutput::Json(v) => serde_json::to_string_pretty(&v).unwrap_or_default(),
    };
    truncate(&s)
}

/// Char-safe truncation to stay under Telegram's message limit.
fn truncate(s: &str) -> String {
    if s.chars().count() > MAX_TG_LEN {
        let cut: String = s.chars().take(MAX_TG_LEN).collect();
        format!("{cut}…")
    } else {
        s.to_string()
    }
}

/// Portfolio PnL summary matching the dashboard: realized (closed) + unrealized
/// (open) across all pools the wallet has touched, sourced from Meteora.
async fn portfolio_text(config: &Config) -> String {
    let wallet = crate::tools::meteora_native::wallet_pubkey_from_env().unwrap_or_default();
    if wallet.is_empty() {
        return "⚠️ wallet not set (MERIDIAN_WALLET)".to_string();
    }
    let _ = config; // wallet comes from env; config reserved for future use
    let pools = crate::tools::dlmm::get_all_wallet_pools(&wallet).await;
    let mut realized = 0.0;
    let mut deposit = 0.0;
    let mut fees = 0.0;
    let mut closed = 0usize;
    let mut wins = 0usize;
    for (pool, name) in &pools {
        if let Some(h) = crate::tools::dlmm::get_pool_history(pool, name, &wallet).await {
            realized += h.pnl_usd;
            deposit += h.deposit_usd;
            fees += h.fees_usd;
            closed += h.closed_count;
            wins += h.win_count;
        }
    }
    let mut unrealized = 0.0;
    for (pool, _) in &pools {
        unrealized += crate::tools::dlmm::get_pool_open_pnl(pool, &wallet).await;
    }
    let total = realized + unrealized;
    let pct = if deposit > 0.0 {
        total / deposit * 100.0
    } else {
        0.0
    };
    let win_rate = if closed > 0 {
        wins as f64 / closed as f64 * 100.0
    } else {
        0.0
    };
    format!(
        "💰 Total PnL: ${total:.2} ({pct:.2}%)\n  realized ${realized:.2} · unrealized ${unrealized:.2}\nFees ${fees:.2} · Win rate {win_rate:.1}% · {closed} closed"
    )
}

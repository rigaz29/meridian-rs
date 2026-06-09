use anyhow::{anyhow, Result};
use axum::{
    extract::{Query, State},
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tower_http::cors::{Any, CorsLayer};

use crate::config::llm_config::LlmCredentials;
use crate::config::{load_config, meridian_data_path, resolve_config_path, save_config, Config};
use crate::cycle::{run_management_cycle, run_screening_cycle};
use crate::lessons::LessonStore;
use crate::llm::{FunctionCall, LlmClient, ToolCall};
use crate::state::pool_memory::PoolMemoryStore;
use crate::state::positions::PositionState;
use crate::tools::blacklist::BlacklistStore;
use crate::tools::executor::{read_recent_decisions_from_path, ToolExecutor};
use crate::tools::screening::Screener;
use crate::tools::wallet::get_wallet_balances;

#[derive(Clone, Debug)]
pub struct WebAppState {
    pub config_path: PathBuf,
    pub state_path: String,
}

impl Default for WebAppState {
    fn default() -> Self {
        let state_path = std::env::var("MERIDIAN_STATE_PATH").unwrap_or_else(|_| {
            meridian_data_path("meridian-state.json")
                .to_string_lossy()
                .into_owned()
        });
        Self {
            config_path: resolve_config_path(None),
            state_path,
        }
    }
}

pub async fn start_web_server() -> anyhow::Result<()> {
    let app = build_router(WebAppState::default());
    let addr = std::env::var("MERIDIAN_WEB_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("[Meridian HyperOS] Running on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn build_router(state: WebAppState) -> Router {
    Router::new()
        .route("/", get(main_page))
        .route("/api/status", get(status))
        .route("/api/positions", get(get_positions))
        .route("/api/balance", get(get_balance))
        .route("/api/candidates", get(get_candidates))
        .route("/api/screening", get(get_candidates))
        .route("/api/decisions", get(get_decisions))
        .route("/api/config", get(get_config).post(post_config))
        .route("/api/control", post(post_control))
        .route("/api/lessons", get(get_lessons))
        .route("/api/performance", get(get_performance))
        .route("/api/blacklist", get(get_blacklist))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
}

async fn main_page() -> Html<&'static str> {
    Html(
        r#"
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Meridian HyperOS</title>
  <script src="https://cdn.tailwindcss.com"></script>
  <style>
    :root { color-scheme: dark; }
    body {
      margin: 0;
      min-height: 100vh;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "SF Pro Display", sans-serif;
      background:
        radial-gradient(circle at 18% 14%, rgba(16,185,129,.28), transparent 34%),
        radial-gradient(circle at 76% 18%, rgba(56,189,248,.24), transparent 32%),
        radial-gradient(circle at 46% 88%, rgba(168,85,247,.18), transparent 35%),
        #020617;
      color: #e5e7eb;
      overflow-x: hidden;
    }
    .glass { background: rgba(15, 23, 42, .68); border: 1px solid rgba(148, 163, 184, .22); box-shadow: 0 24px 80px rgba(0,0,0,.35); backdrop-filter: blur(18px); }
    .dock { background: rgba(2, 6, 23, .78); border: 1px solid rgba(148, 163, 184, .2); backdrop-filter: blur(22px); }
    .app { min-height: 260px; }
    .mono { font-family: "JetBrains Mono", "SF Mono", ui-monospace, monospace; }
    button { transition: transform .15s ease, background .15s ease, border-color .15s ease; }
    button:hover { transform: translateY(-1px); }
  </style>
</head>
<body>
  <main class="p-6 lg:p-8 max-w-[1800px] mx-auto">
    <header class="flex flex-col xl:flex-row xl:items-end xl:justify-between gap-4 mb-8">
      <section>
        <p class="text-emerald-300 tracking-[0.35em] text-xs uppercase mono">Local DLMM control surface</p>
        <h1 class="text-5xl lg:text-7xl font-black tracking-tight mt-2">Meridian HyperOS</h1>
        <p class="text-slate-300 mt-3 max-w-3xl">Rust-native replacement for Telegram controls: live state, candidate radar, cycle logs, manual actions, config editing, lessons, performance, and blacklist management.</p>
      </section>
      <nav class="dock rounded-3xl p-2 grid grid-cols-2 sm:grid-cols-5 gap-2 text-sm">
        <button onclick="refreshAll()" class="px-4 py-3 rounded-2xl bg-emerald-500/20 border border-emerald-400/30">Refresh</button>
        <button onclick="runControl('screen')" class="px-4 py-3 rounded-2xl bg-cyan-500/20 border border-cyan-400/30">Run Screen</button>
        <button onclick="runControl('manage')" class="px-4 py-3 rounded-2xl bg-violet-500/20 border border-violet-400/30">Run Manage</button>
        <button onclick="openConfigPatch()" class="px-4 py-3 rounded-2xl bg-amber-500/20 border border-amber-400/30">Config Patch</button>
        <button onclick="clearLog()" class="px-4 py-3 rounded-2xl bg-slate-500/20 border border-slate-400/30">Clear Log</button>
      </nav>
    </header>

    <section class="grid grid-cols-1 xl:grid-cols-4 gap-5 mb-5">
      <div class="glass rounded-[2rem] p-5 app" data-app="dashboard">
        <div class="text-xs mono text-emerald-300 uppercase tracking-[.22em]">Dashboard</div>
        <h2 class="text-2xl font-bold mt-2">Runtime Status</h2>
        <pre id="status" class="mono text-xs whitespace-pre-wrap text-slate-300 mt-4">Loading...</pre>
      </div>
      <div class="glass rounded-[2rem] p-5 app" data-app="positions">
        <div class="text-xs mono text-cyan-300 uppercase tracking-[.22em]">Live Positions</div>
        <h2 class="text-2xl font-bold mt-2">Positions</h2>
        <div id="positions" class="space-y-3 mt-4 text-sm text-slate-300">Loading...</div>
      </div>
      <div class="glass rounded-[2rem] p-5 app" data-app="candidates">
        <div class="text-xs mono text-fuchsia-300 uppercase tracking-[.22em]">Candidate Radar</div>
        <h2 class="text-2xl font-bold mt-2">Candidates</h2>
        <div id="candidates" class="space-y-3 mt-4 text-sm text-slate-300">Click refresh to scan.</div>
      </div>
      <div class="glass rounded-[2rem] p-5 app" data-app="balance">
        <div class="text-xs mono text-amber-300 uppercase tracking-[.22em]">Wallet</div>
        <h2 class="text-2xl font-bold mt-2">Balances</h2>
        <input id="wallet" placeholder="Wallet address" class="mt-4 w-full bg-slate-950/70 border border-slate-700 rounded-2xl px-3 py-2 text-sm outline-none">
        <button onclick="loadBalance()" class="mt-3 w-full rounded-2xl bg-amber-500/20 border border-amber-400/30 py-2">Load Balance</button>
        <pre id="balance" class="mono text-xs whitespace-pre-wrap text-slate-300 mt-3">Wallet required.</pre>
      </div>
    </section>

    <section class="grid grid-cols-1 xl:grid-cols-3 gap-5 mb-5">
      <div class="glass rounded-[2rem] p-5 app xl:col-span-2" data-app="controls">
        <div class="text-xs mono text-rose-300 uppercase tracking-[.22em]">Manual Controls</div>
        <h2 class="text-2xl font-bold mt-2">Deploy / Claim / Close / Swap / Cycle Controls</h2>
        <p class="text-slate-400 text-sm mt-2">Actions are sent to <span class="mono">/api/control</span> and respect Rust config dry-run guardrails.</p>
        <div class="grid md:grid-cols-4 gap-3 mt-4">
          <input id="control-action" value="deploy_position" class="bg-slate-950/70 border border-slate-700 rounded-2xl px-3 py-2 text-sm" placeholder="action">
          <input id="control-pool" class="bg-slate-950/70 border border-slate-700 rounded-2xl px-3 py-2 text-sm" placeholder="pool">
          <input id="control-position" class="bg-slate-950/70 border border-slate-700 rounded-2xl px-3 py-2 text-sm" placeholder="position_id">
          <input id="control-amount" class="bg-slate-950/70 border border-slate-700 rounded-2xl px-3 py-2 text-sm" placeholder="amount_sol">
        </div>
        <button onclick="runManualControl()" class="mt-3 px-4 py-2 rounded-2xl bg-rose-500/20 border border-rose-400/30">Execute Manual Control</button>
        <pre id="control-result" class="mono text-xs whitespace-pre-wrap mt-4 text-slate-300">No action yet.</pre>
      </div>
      <div class="glass rounded-[2rem] p-5 app" data-app="cycle-log">
        <div class="text-xs mono text-lime-300 uppercase tracking-[.22em]">Cycle Logs</div>
        <h2 class="text-2xl font-bold mt-2">Control Log</h2>
        <div id="cycle-log" class="mono text-xs h-[260px] overflow-auto text-slate-300 mt-4">Booting HyperOS...</div>
      </div>
    </section>

    <section class="grid grid-cols-1 xl:grid-cols-4 gap-5">
      <div class="glass rounded-[2rem] p-5 app" data-app="decisions">
        <div class="text-xs mono text-sky-300 uppercase tracking-[.22em]">Recent Decisions</div>
        <h2 class="text-2xl font-bold mt-2">Decision Log</h2>
        <pre id="decisions" class="mono text-xs whitespace-pre-wrap text-slate-300 mt-4">Loading...</pre>
      </div>
      <div class="glass rounded-[2rem] p-5 app" data-app="config">
        <div class="text-xs mono text-orange-300 uppercase tracking-[.22em]">Config Editor</div>
        <h2 class="text-2xl font-bold mt-2">Patch Config</h2>
        <input id="config-path" value="management.deployAmountSol" class="mt-4 w-full bg-slate-950/70 border border-slate-700 rounded-2xl px-3 py-2 text-sm">
        <input id="config-value" value="0.1" class="mt-3 w-full bg-slate-950/70 border border-slate-700 rounded-2xl px-3 py-2 text-sm">
        <button onclick="patchConfig()" class="mt-3 w-full rounded-2xl bg-orange-500/20 border border-orange-400/30 py-2">Save Patch</button>
        <pre id="config" class="mono text-xs whitespace-pre-wrap text-slate-300 mt-3">Loading...</pre>
      </div>
      <div class="glass rounded-[2rem] p-5 app" data-app="lessons">
        <div class="text-xs mono text-purple-300 uppercase tracking-[.22em]">Lessons</div>
        <h2 class="text-2xl font-bold mt-2">Lessons & Performance</h2>
        <pre id="lessons" class="mono text-xs whitespace-pre-wrap text-slate-300 mt-4">Loading...</pre>
        <pre id="performance" class="mono text-xs whitespace-pre-wrap text-slate-300 mt-4">Loading...</pre>
      </div>
      <div class="glass rounded-[2rem] p-5 app" data-app="blacklist">
        <div class="text-xs mono text-red-300 uppercase tracking-[.22em]">Blacklist</div>
        <h2 class="text-2xl font-bold mt-2">Token / Dev Blocks</h2>
        <pre id="blacklist" class="mono text-xs whitespace-pre-wrap text-slate-300 mt-4">Loading...</pre>
      </div>
    </section>
  </main>

<script>
const pretty = (v) => JSON.stringify(v, null, 2);
const log = (msg) => { const el = document.getElementById('cycle-log'); el.innerHTML += `<div>${new Date().toLocaleTimeString()} ${msg}</div>`; el.scrollTop = el.scrollHeight; };
async function api(path, options={}) { const res = await fetch(path, options); return await res.json(); }
async function refreshAll() {
  log('refreshing live state');
  const [status, positions, decisions, config, lessons, performance, blacklist] = await Promise.all([
    api('/api/status'), api('/api/positions'), api('/api/decisions'), api('/api/config'), api('/api/lessons'), api('/api/performance'), api('/api/blacklist')
  ]);
  document.getElementById('status').textContent = pretty(status);
  document.getElementById('positions').textContent = pretty(positions);
  document.getElementById('decisions').textContent = pretty(decisions);
  document.getElementById('config').textContent = pretty(config.data || config);
  document.getElementById('lessons').textContent = pretty(lessons);
  document.getElementById('performance').textContent = pretty(performance);
  document.getElementById('blacklist').textContent = pretty(blacklist);
}
async function loadCandidates() {
  const data = await api('/api/candidates?limit=5');
  document.getElementById('candidates').textContent = pretty(data);
}
async function loadBalance() {
  const wallet = encodeURIComponent(document.getElementById('wallet').value.trim());
  const data = await api('/api/balance?wallet=' + wallet);
  document.getElementById('balance').textContent = pretty(data);
}
async function runControl(action) {
  log('control: ' + action);
  const data = await api('/api/control', { method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify({ action, wallet_sol: 0 }) });
  document.getElementById('control-result').textContent = pretty(data);
  await refreshAll();
}
async function runManualControl() {
  const action = document.getElementById('control-action').value.trim();
  const pool = document.getElementById('control-pool').value.trim();
  const position_id = document.getElementById('control-position').value.trim();
  const amount = Number(document.getElementById('control-amount').value || 0);
  const args = { pool_address: pool, pool, position_id, amount_sol: amount, dry_run: true, skip_swap: true };
  const data = await api('/api/control', { method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify({ action, args }) });
  document.getElementById('control-result').textContent = pretty(data);
  log('manual action finished: ' + action);
}
async function patchConfig() {
  let raw = document.getElementById('config-value').value;
  let value; try { value = JSON.parse(raw); } catch { value = raw; }
  const body = { path: document.getElementById('config-path').value.trim(), value };
  const data = await api('/api/config', { method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify(body) });
  document.getElementById('config').textContent = pretty(data);
}
function openConfigPatch(){ document.getElementById('config-path').focus(); }
function clearLog(){ document.getElementById('cycle-log').innerHTML = ''; }
setInterval(refreshAll, 15000);
refreshAll();
loadCandidates();
</script>
</body>
</html>
    "#,
    )
}

#[derive(Debug, Deserialize)]
struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct BalanceQuery {
    wallet: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ControlRequest {
    action: String,
    #[serde(default)]
    args: Value,
    #[serde(default)]
    wallet: Option<String>,
    #[serde(default)]
    wallet_sol: Option<f64>,
}

async fn status(State(state): State<WebAppState>) -> Json<Value> {
    let config = load_web_config(&state);
    Json(match config {
        Ok(config) => json_ok("status", web_status_snapshot(&config, &state.state_path)),
        Err(error) => json_error("status", error),
    })
}

async fn get_positions(State(state): State<WebAppState>) -> Json<Value> {
    Json(json_ok("positions", positions_payload(&state.state_path)))
}

async fn get_balance(
    State(state): State<WebAppState>,
    Query(query): Query<BalanceQuery>,
) -> Json<Value> {
    let wallet = query.wallet.unwrap_or_default();
    if wallet.trim().is_empty() {
        return Json(json!({
            "success": true,
            "command": "balance",
            "data": {"available": false, "reason": "wallet query parameter required"}
        }));
    }
    let config = match load_web_config(&state) {
        Ok(config) => config,
        Err(error) => return Json(json_error("balance", error)),
    };
    let rpc_url = config.api.helius_rpc_url.as_deref().unwrap_or_default();
    let helius_api_key = config.api.helius_api_key.as_deref().unwrap_or_default();
    Json(
        match get_wallet_balances(rpc_url, &wallet, helius_api_key).await {
            Ok(balance) => json!({"success": true, "command": "balance", "data": balance}),
            Err(error) => json_error("balance", error),
        },
    )
}

async fn get_candidates(
    State(state): State<WebAppState>,
    Query(query): Query<LimitQuery>,
) -> Json<Value> {
    let config = match load_web_config(&state) {
        Ok(config) => config,
        Err(error) => return Json(json_error("candidates", error)),
    };
    let screener = Screener::new();
    Json(
        match screener
            .get_top_candidates_with_rejections(&config.screening, query.limit.unwrap_or(5))
            .await
        {
            Ok(result) => json!({"success": true, "command": "candidates", "data": result}),
            Err(error) => json_error("candidates", error),
        },
    )
}

async fn get_decisions(State(state): State<WebAppState>) -> Json<Value> {
    Json(json_ok(
        "decisions",
        recent_decisions_payload(&state.state_path, 25),
    ))
}

async fn get_config(State(state): State<WebAppState>) -> Json<Value> {
    Json(match load_web_config(&state) {
        Ok(config) => json!({
            "success": true,
            "command": "config",
            "data": redact_sensitive_values(serde_json::to_value(config).unwrap_or(Value::Null)),
            "path": state.config_path.display().to_string(),
        }),
        Err(error) => json_error("config", error),
    })
}

async fn post_config(State(state): State<WebAppState>, Json(body): Json<Value>) -> Json<Value> {
    Json(match apply_config_patch(&state.config_path, &body) {
        Ok(result) => json!({"success": true, "command": "config", "data": result}),
        Err(error) => json_error("config", error),
    })
}

async fn post_control(
    State(state): State<WebAppState>,
    Json(request): Json<ControlRequest>,
) -> Json<Value> {
    Json(match run_control_action(&state, request).await {
        Ok(result) => json!({"success": true, "command": "control", "data": result}),
        Err(error) => json_error("control", error),
    })
}

async fn get_lessons(State(state): State<WebAppState>) -> Json<Value> {
    Json(json_ok("lessons", lessons_payload(&state.state_path)))
}

async fn get_performance(State(state): State<WebAppState>) -> Json<Value> {
    Json(json_ok(
        "performance",
        performance_payload(&state.state_path),
    ))
}

async fn get_blacklist() -> Json<Value> {
    Json(json_ok("blacklist", blacklist_payload()))
}

fn load_web_config(state: &WebAppState) -> Result<Config> {
    load_config(state.config_path.to_str())
}

fn json_ok(command: &str, data: Result<Value>) -> Value {
    match data {
        Ok(data) => json!({"success": true, "command": command, "data": data}),
        Err(error) => json_error(command, error),
    }
}

fn json_error(command: &str, error: impl std::fmt::Display) -> Value {
    json!({"success": false, "command": command, "error": error.to_string()})
}

pub fn web_status_snapshot(config: &Config, state_path: &str) -> Result<Value> {
    let positions = positions_payload(state_path)?;
    let recent_decisions = recent_decisions_payload(state_path, 10)?;
    Ok(json!({
        "status": "running",
        "dry_run": config.dry_run,
        "active_positions": positions["active_count"].clone(),
        "positions": positions["positions"].clone(),
        "recent_events": positions["recent_events"].clone(),
        "recent_decisions": recent_decisions["decisions"].clone(),
        "state_path": state_path,
        "data_dir": data_dir_for_state(state_path).display().to_string(),
        "config": redact_sensitive_values(serde_json::to_value(config)?),
        "schedule": {
            "managementIntervalMin": config.schedule.management_interval_min,
            "screeningIntervalMin": config.schedule.screening_interval_min,
            "pnlPollIntervalSecs": config.schedule.pnl_poll_interval_secs,
        },
    }))
}

fn positions_payload(state_path: &str) -> Result<Value> {
    let state = PositionState::load(state_path)?;
    let mut positions = state.positions.values().cloned().collect::<Vec<_>>();
    positions.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(json!({
        "active_count": state.count_active(),
        "total_count": positions.len(),
        "positions": positions,
        "recent_events": state.recent_events,
        "last_updated": state.last_updated,
        "path": state_path,
    }))
}

fn recent_decisions_payload(state_path: &str, limit: usize) -> Result<Value> {
    let path = data_dir_for_state(state_path).join("decision-log.json");
    let decisions = read_recent_decisions_from_path(&path, limit)?;
    Ok(json!({
        "count": decisions.len(),
        "decisions": decisions,
        "path": path.display().to_string(),
    }))
}

fn lessons_payload(state_path: &str) -> Result<Value> {
    let path = data_dir_for_state(state_path).join("lessons.json");
    let store = LessonStore::load(
        path.to_str()
            .ok_or_else(|| anyhow!("lesson path is not UTF-8"))?,
    )?;
    Ok(json!({
        "count": store.lessons.len(),
        "lessons": store.lessons,
        "path": path.display().to_string(),
    }))
}

fn performance_payload(state_path: &str) -> Result<Value> {
    let path = data_dir_for_state(state_path).join("lessons.json");
    let store = LessonStore::load(
        path.to_str()
            .ok_or_else(|| anyhow!("lesson path is not UTF-8"))?,
    )?;
    let history = store.get_performance_history(24.0, 50);
    Ok(json!({
        "history": history,
        "raw_count": store.performance.len(),
        "path": path.display().to_string(),
    }))
}

fn blacklist_payload() -> Result<Value> {
    let store = BlacklistStore::load()?;
    Ok(json!({
        "tokens": store.list_blacklist(),
        "blocked_devs": store.list_blocked_devs(),
    }))
}

pub fn apply_config_patch(config_path: &Path, patch: &Value) -> Result<Value> {
    let path = patch
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| anyhow!("config patch requires non-empty path"))?;
    let new_value = patch
        .get("value")
        .cloned()
        .ok_or_else(|| anyhow!("config patch requires value"))?;

    let config_path_str = config_path
        .to_str()
        .ok_or_else(|| anyhow!("config path is not UTF-8"))?;
    let mut value = if config_path.exists() {
        serde_json::from_str::<Value>(&std::fs::read_to_string(config_path)?)?
    } else {
        serde_json::to_value(Config::default())?
    };
    set_nested_value(&mut value, path, new_value.clone())?;
    let updated: Config = serde_json::from_value(value)
        .map_err(|error| anyhow!("updated config is invalid for '{}': {}", path, error))?;
    save_config(&updated, Some(config_path_str))?;

    Ok(json!({
        "ok": true,
        "path": path,
        "value": new_value,
        "config": redact_sensitive_values(serde_json::to_value(updated)?),
    }))
}

fn set_nested_value(target: &mut Value, path: &str, new_value: Value) -> Result<()> {
    let parts = path
        .split('.')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return Err(anyhow!("empty config path"));
    }
    let mut cursor = target;
    for segment in &parts[..parts.len() - 1] {
        cursor = cursor
            .get_mut(*segment)
            .ok_or_else(|| anyhow!("unknown config parent '{}'", segment))?;
    }
    let leaf = parts.last().expect("non-empty path");
    let object = cursor
        .as_object_mut()
        .ok_or_else(|| anyhow!("config parent for '{}' is not an object", path))?;
    if !object.contains_key(*leaf) {
        return Err(anyhow!("unknown config key '{}'", path));
    }
    object.insert((*leaf).to_string(), new_value);
    Ok(())
}

async fn run_control_action(state: &WebAppState, request: ControlRequest) -> Result<Value> {
    let config = load_web_config(state)?;
    let mut positions = PositionState::load(&state.state_path)?;
    let pool_memory_path = data_dir_for_state(&state.state_path).join("pool-memory.json");
    let mut pool_memory = PoolMemoryStore::load(
        pool_memory_path
            .to_str()
            .ok_or_else(|| anyhow!("pool memory path is not UTF-8"))?,
    )?;
    let wallet = request
        .wallet
        .or_else(|| std::env::var("MERIDIAN_WALLET").ok())
        .unwrap_or_default();

    let result = match request.action.as_str() {
        "screen" | "screening" => {
            let creds = LlmCredentials::from_env_or_config(
                Some(&config.llm.base_url),
                config.llm.api_key.as_deref(),
            );
            let llm = LlmClient::new(&creds.api_key, &creds.base_url);
            let wallet_sol = request.wallet_sol.unwrap_or(0.0);
            json!({
                "action": "screen",
                "result": run_screening_cycle(&config, &llm, &mut positions, &mut pool_memory, wallet_sol, &wallet).await?,
            })
        }
        "manage" | "management" => {
            let creds = LlmCredentials::from_env_or_config(
                Some(&config.llm.base_url),
                config.llm.api_key.as_deref(),
            );
            let llm = LlmClient::new(&creds.api_key, &creds.base_url);
            json!({
                "action": "manage",
                "result": run_management_cycle(&config, &llm, &mut positions, &mut pool_memory, &wallet).await?,
            })
        }
        tool_name => {
            let args = if request.args.is_null() {
                json!({})
            } else {
                request.args
            };
            let call = ToolCall {
                id: "web-control".to_string(),
                call_type: "function".to_string(),
                function: FunctionCall {
                    name: tool_name.to_string(),
                    arguments: serde_json::to_string(&args)?,
                },
            };
            let mut executor = ToolExecutor::new(&wallet);
            let (output, _is_error) = executor
                .execute(&call, &config, &mut positions, &mut pool_memory)
                .await;
            json!({
                "action": tool_name,
                "args": args,
                "result": serde_json::from_str::<Value>(&output).unwrap_or(Value::String(output)),
            })
        }
    };

    positions.save(&state.state_path)?;
    pool_memory.save(
        pool_memory_path
            .to_str()
            .ok_or_else(|| anyhow!("pool memory path is not UTF-8"))?,
    )?;
    Ok(result)
}

fn data_dir_for_state(state_path: &str) -> PathBuf {
    Path::new(state_path)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn redact_sensitive_values(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    if is_sensitive_key(&key) {
                        (key, Value::String("***redacted***".to_string()))
                    } else {
                        (key, redact_sensitive_values(value))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => {
            Value::Array(values.into_iter().map(redact_sensitive_values).collect())
        }
        other => other,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    normalized.contains("apikey")
        || normalized.contains("api_key")
        || normalized.contains("token")
        || normalized.contains("secret")
        || normalized.contains("private")
        || normalized.contains("password")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::state::positions::{PositionState, TrackedPosition};
    use serde_json::json;

    fn unique_test_dir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        std::env::temp_dir().join(format!("meridian-rs-web-{}-{}", label, nanos))
    }

    #[tokio::test]
    async fn main_page_contains_phase6_control_surface_apps() {
        let html = main_page().await.0;

        for label in [
            "Meridian HyperOS",
            "Live Positions",
            "Candidate Radar",
            "Cycle Logs",
            "Recent Decisions",
            "Manual Controls",
            "Config Editor",
            "Lessons",
            "Performance",
            "Blacklist",
        ] {
            assert!(html.contains(label), "missing UI label: {label}");
        }
        assert!(html.contains("/api/control"));
        assert!(html.contains("/api/config"));
    }

    #[test]
    fn web_status_snapshot_reads_live_state_decisions_and_redacts_config() {
        let dir = unique_test_dir("status-snapshot");
        std::fs::create_dir_all(&dir).expect("test dir");
        let state_path = dir.join("state.json");

        let mut state = PositionState::default();
        state.positions.insert(
            "pos-1".to_string(),
            TrackedPosition {
                id: "pos-1".to_string(),
                pool_address: "Pool111".to_string(),
                base_mint: "Mint111".to_string(),
                base_symbol: Some("MOON".to_string()),
                amount_sol: 1.25,
                ..TrackedPosition::default()
            },
        );
        state
            .save(state_path.to_str().unwrap())
            .expect("state save");
        std::fs::write(
            dir.join("decision-log.json"),
            serde_json::to_string_pretty(&json!([
                {"tool":"deploy_position","success":true,"timestamp":"now"}
            ]))
            .unwrap(),
        )
        .expect("decision log");

        let mut config = Config::default();
        config.llm.api_key = Some("secret-key".to_string());
        let snapshot =
            web_status_snapshot(&config, state_path.to_str().unwrap()).expect("snapshot");

        assert_eq!(snapshot["active_positions"], 1);
        assert_eq!(snapshot["positions"][0]["base_symbol"], "MOON");
        assert_eq!(snapshot["recent_decisions"][0]["tool"], "deploy_position");
        assert_eq!(snapshot["config"]["llm"]["apiKey"], "***redacted***");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_patch_updates_nested_values_without_overwriting_the_whole_file() {
        let dir = unique_test_dir("config-patch");
        std::fs::create_dir_all(&dir).expect("test dir");
        let config_path = dir.join("user-config.json");
        let config = Config {
            dry_run: true,
            ..Config::default()
        };
        crate::config::save_config(&config, Some(config_path.to_str().unwrap()))
            .expect("save config");

        let result = apply_config_patch(
            &config_path,
            &json!({"path":"management.deployAmountSol","value":0.42}),
        )
        .expect("patch config");
        let updated =
            crate::config::load_config(Some(config_path.to_str().unwrap())).expect("load config");

        assert_eq!(result["path"], "management.deployAmountSol");
        assert_eq!(updated.management.deploy_amount_sol, 0.42);
        assert_eq!(result["config"]["dryRun"], true);
        std::fs::remove_dir_all(&dir).ok();
    }
}

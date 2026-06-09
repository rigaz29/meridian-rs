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
  <style>
    :root {
      color-scheme: dark;
      --bg: #050816;
      --panel: rgba(12, 18, 32, .78);
      --panel-strong: rgba(15, 23, 42, .92);
      --line: rgba(148, 163, 184, .18);
      --muted: #94a3b8;
      --text: #e5edf7;
      --cyan: #38bdf8;
      --emerald: #34d399;
      --violet: #a78bfa;
      --amber: #fbbf24;
      --rose: #fb7185;
      --orange: #fb923c;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      min-height: 100vh;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "SF Pro Display", sans-serif;
      background:
        radial-gradient(circle at 15% 12%, rgba(34,211,238,.22), transparent 30%),
        radial-gradient(circle at 86% 14%, rgba(167,139,250,.18), transparent 30%),
        radial-gradient(circle at 55% 92%, rgba(16,185,129,.16), transparent 34%),
        linear-gradient(135deg, #020617, #07111f 48%, #07031a);
      color: var(--text);
      overflow-x: hidden;
    }
    body::before {
      content: "";
      position: fixed;
      inset: 0;
      pointer-events: none;
      background-image: linear-gradient(rgba(148,163,184,.05) 1px, transparent 1px), linear-gradient(90deg, rgba(148,163,184,.05) 1px, transparent 1px);
      background-size: 44px 44px;
      mask-image: linear-gradient(to bottom, black, transparent 88%);
    }
    main { width: min(1760px, calc(100vw - 48px)); margin: 0 auto; padding: 32px 0 48px; position: relative; z-index: 1; }
    .hero { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 24px; align-items: end; margin-bottom: 24px; }
    .eyebrow { color: var(--emerald); letter-spacing: .34em; text-transform: uppercase; font: 700 11px/1.2 "SF Mono", ui-monospace, monospace; }
    h1 { margin: 8px 0 8px; font-size: clamp(42px, 7vw, 86px); line-height: .92; letter-spacing: -.07em; }
    .subtitle { max-width: 780px; color: #cbd5e1; font-size: 17px; line-height: 1.65; margin: 0; }
    .toolbar { display: grid; grid-template-columns: repeat(5, minmax(104px, 1fr)); gap: 10px; padding: 8px; border: 1px solid var(--line); border-radius: 28px; background: rgba(2,6,23,.62); backdrop-filter: blur(18px); box-shadow: 0 24px 70px rgba(0,0,0,.3); }
    button, select, input { font: inherit; }
    button { border: 1px solid var(--line); color: var(--text); background: rgba(15,23,42,.82); border-radius: 18px; padding: 12px 16px; cursor: pointer; transition: transform .15s ease, border-color .15s ease, background .15s ease; }
    button:hover { transform: translateY(-1px); border-color: rgba(56,189,248,.55); background: rgba(30,41,59,.88); }
    button:disabled { opacity: .55; cursor: wait; transform: none; }
    .btn-emerald { border-color: rgba(52,211,153,.35); background: rgba(6,78,59,.32); }
    .btn-cyan { border-color: rgba(56,189,248,.35); background: rgba(8,47,73,.36); }
    .btn-violet { border-color: rgba(167,139,250,.35); background: rgba(76,29,149,.30); }
    .btn-amber { border-color: rgba(251,191,36,.35); background: rgba(120,53,15,.28); }
    .btn-rose { border-color: rgba(251,113,133,.35); background: rgba(136,19,55,.28); }
    .overview { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 14px; margin-bottom: 18px; }
    .metric { border: 1px solid var(--line); border-radius: 26px; padding: 18px; background: rgba(15,23,42,.56); box-shadow: inset 0 1px rgba(255,255,255,.04); }
    .metric span { color: var(--muted); font-size: 12px; text-transform: uppercase; letter-spacing: .18em; }
    .metric strong { display: block; margin-top: 8px; font-size: 28px; letter-spacing: -.04em; }
    .grid { display: grid; grid-template-columns: repeat(12, minmax(0, 1fr)); gap: 18px; }
    .panel { grid-column: span 3; min-height: 260px; border: 1px solid var(--line); border-radius: 30px; background: var(--panel); box-shadow: 0 24px 90px rgba(0,0,0,.26), inset 0 1px rgba(255,255,255,.05); backdrop-filter: blur(18px); overflow: hidden; }
    .panel.wide { grid-column: span 8; }
    .panel.medium { grid-column: span 4; }
    .panel.tall .panel-body { max-height: 560px; }
    .panel-header { padding: 18px 20px 12px; border-bottom: 1px solid rgba(148,163,184,.10); }
    .panel-header .label { font: 800 11px/1.2 "SF Mono", ui-monospace, monospace; letter-spacing: .24em; text-transform: uppercase; color: var(--cyan); }
    .panel-header h2 { margin: 7px 0 0; font-size: 24px; line-height: 1.1; letter-spacing: -.04em; }
    .panel-body { padding: 18px 20px 22px; max-height: 380px; overflow: auto; color: #dbeafe; }
    .panel-body::-webkit-scrollbar { width: 9px; height: 9px; }
    .panel-body::-webkit-scrollbar-thumb { background: rgba(148,163,184,.26); border-radius: 999px; }
    .kv { display: grid; grid-template-columns: minmax(110px, .8fr) minmax(0, 1.2fr); gap: 10px; padding: 10px 0; border-bottom: 1px solid rgba(148,163,184,.08); }
    .kv:last-child { border-bottom: 0; }
    .kv span { color: var(--muted); font-size: 12px; text-transform: uppercase; letter-spacing: .12em; }
    .kv strong { min-width: 0; overflow-wrap: anywhere; }
    .pill { display: inline-flex; align-items: center; gap: 7px; padding: 6px 10px; border-radius: 999px; border: 1px solid var(--line); color: #dbeafe; background: rgba(15,23,42,.75); font: 700 12px/1 "SF Mono", ui-monospace, monospace; }
    .pill.ok { color: #bbf7d0; border-color: rgba(52,211,153,.35); background: rgba(6,78,59,.26); }
    .pill.warn { color: #fde68a; border-color: rgba(251,191,36,.35); background: rgba(120,53,15,.24); }
    .pill.bad { color: #fecdd3; border-color: rgba(251,113,133,.35); background: rgba(136,19,55,.24); }
    .list { display: grid; gap: 10px; }
    .item { border: 1px solid rgba(148,163,184,.12); border-radius: 18px; padding: 12px; background: rgba(2,6,23,.35); }
    .item-title { display: flex; justify-content: space-between; gap: 10px; font-weight: 800; }
    .item-sub { color: var(--muted); margin-top: 6px; font-size: 13px; overflow-wrap: anywhere; }
    .empty { color: var(--muted); border: 1px dashed rgba(148,163,184,.22); border-radius: 18px; padding: 18px; background: rgba(15,23,42,.34); }
    .form-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; }
    .field label { display: block; color: var(--muted); font-size: 11px; letter-spacing: .12em; text-transform: uppercase; margin-bottom: 7px; }
    input, select { width: 100%; color: var(--text); background: rgba(2,6,23,.68); border: 1px solid rgba(148,163,184,.22); border-radius: 16px; padding: 12px 13px; outline: none; }
    input:focus, select:focus { border-color: rgba(56,189,248,.65); box-shadow: 0 0 0 3px rgba(56,189,248,.12); }
    .result, .log { margin-top: 14px; padding: 14px; border-radius: 18px; background: rgba(2,6,23,.44); border: 1px solid rgba(148,163,184,.10); font: 12px/1.55 "SF Mono", ui-monospace, monospace; white-space: pre-wrap; overflow: auto; max-height: 240px; }
    .log-line { padding: 5px 0; border-bottom: 1px solid rgba(148,163,184,.07); }
    .muted { color: var(--muted); }
    .accent-emerald { color: var(--emerald) !important; } .accent-violet { color: var(--violet) !important; } .accent-amber { color: var(--amber) !important; } .accent-rose { color: var(--rose) !important; } .accent-orange { color: var(--orange) !important; }
    @media (max-width: 1180px) { .hero { grid-template-columns: 1fr; } .toolbar, .overview { grid-template-columns: repeat(2, minmax(0, 1fr)); } .panel, .panel.medium, .panel.wide { grid-column: span 6; } }
    @media (max-width: 760px) { main { width: min(100vw - 24px, 1760px); padding-top: 18px; } .toolbar, .overview, .form-grid { grid-template-columns: 1fr; } .grid { grid-template-columns: 1fr; } .panel, .panel.medium, .panel.wide { grid-column: span 1; } }
  </style>
</head>
<body>
  <main>
    <header class="hero">
      <section>
        <div class="eyebrow">Local DLMM control surface</div>
        <h1>Meridian HyperOS</h1>
        <p class="subtitle">Operator-ready dashboard for live state, candidate radar, guarded controls, config patches, lessons, performance, and readable execution logs.</p>
      </section>
      <nav class="toolbar" aria-label="Quick actions">
        <button id="refresh-btn" class="btn-emerald" onclick="refreshAll()">Refresh</button>
        <button class="btn-cyan" onclick="runControl('screen')">Run Screen</button>
        <button class="btn-violet" onclick="runControl('manage')">Run Manage</button>
        <button class="btn-amber" onclick="openConfigPatch()">Config Patch</button>
        <button onclick="clearLog()">Clear Log</button>
      </nav>
    </header>

    <section class="overview" aria-label="Runtime summary">
      <div class="metric"><span>Runtime</span><strong id="metric-runtime">Loading</strong></div>
      <div class="metric"><span>Mode</span><strong id="metric-mode">—</strong></div>
      <div class="metric"><span>Positions</span><strong id="metric-positions">0</strong></div>
      <div class="metric"><span>Candidates</span><strong id="metric-candidates">0</strong></div>
    </section>

    <section class="grid">
      <article class="panel medium" data-panel="dashboard">
        <header class="panel-header"><div class="label accent-emerald">Dashboard</div><h2>Runtime Status</h2></header>
        <div id="status" class="panel-body">Loading runtime state…</div>
      </article>

      <article class="panel medium" data-panel="positions">
        <header class="panel-header"><div class="label">Live Positions</div><h2>Positions</h2></header>
        <div id="positions" class="panel-body">Loading positions…</div>
      </article>

      <article class="panel medium" data-panel="candidates">
        <header class="panel-header"><div class="label accent-violet">Candidate Radar</div><h2>Candidates</h2></header>
        <div id="candidates" class="panel-body">Loading candidates…</div>
      </article>

      <article class="panel medium" data-panel="balance">
        <header class="panel-header"><div class="label accent-amber">Wallet</div><h2>Balances</h2></header>
        <div class="panel-body">
          <div class="field"><label for="wallet">Wallet address</label><input id="wallet" placeholder="Paste wallet address"></div>
          <button class="btn-amber" style="width:100%;margin-top:12px" onclick="loadBalance()">Load Balance</button>
          <div id="balance" class="result">Wallet required.</div>
        </div>
      </article>

      <article class="panel wide" data-panel="controls">
        <header class="panel-header"><div class="label accent-rose">Manual Controls</div><h2>Deploy / Claim / Close / Swap / Cycle Controls</h2></header>
        <div class="panel-body">
          <p class="muted">Actions are sent to <span class="pill">/api/control</span> and stay behind Rust dry-run guardrails unless config explicitly allows live execution.</p>
          <div class="form-grid">
            <div class="field"><label for="control-action">Action</label><select id="control-action"><option>deploy_position</option><option>claim_fees</option><option>close_position</option><option>swap_token</option><option>screen</option><option>manage</option></select></div>
            <div class="field"><label for="control-pool">Pool</label><input id="control-pool" placeholder="pool"></div>
            <div class="field"><label for="control-position">Position</label><input id="control-position" placeholder="position_id"></div>
            <div class="field"><label for="control-amount">Amount SOL</label><input id="control-amount" placeholder="0.10" inputmode="decimal"></div>
          </div>
          <button class="btn-rose" style="margin-top:12px" onclick="runManualControl()">Execute Manual Control</button>
          <div id="control-result" class="result">No action yet.</div>
        </div>
      </article>

      <article class="panel medium" data-panel="cycle-log">
        <header class="panel-header"><div class="label accent-emerald">Cycle Logs</div><h2>Control Log</h2></header>
        <div id="cycle-log" class="panel-body log"><div class="log-line">Booting HyperOS…</div></div>
      </article>

      <article class="panel medium" data-panel="decisions">
        <header class="panel-header"><div class="label">Recent Decisions</div><h2>Decision Log</h2></header>
        <div id="decisions" class="panel-body">Loading decisions…</div>
      </article>

      <article class="panel medium tall" data-panel="config">
        <header class="panel-header"><div class="label accent-orange">Config Editor</div><h2>Patch Config</h2></header>
        <div class="panel-body">
          <div class="field"><label for="config-path">Config path</label><input id="config-path" value="management.deployAmountSol"></div>
          <div class="field" style="margin-top:10px"><label for="config-value">New value</label><input id="config-value" value="0.1"></div>
          <button class="btn-amber" style="width:100%;margin-top:12px" onclick="patchConfig()">Save Patch</button>
          <div id="config" class="result">Loading config summary…</div>
        </div>
      </article>

      <article class="panel medium" data-panel="lessons">
        <header class="panel-header"><div class="label accent-violet">Lessons</div><h2>Lessons & Performance</h2></header>
        <div id="lessons" class="panel-body">Loading lessons…</div>
        <div id="performance" class="panel-body" style="border-top:1px solid rgba(148,163,184,.10)">Loading performance…</div>
      </article>

      <article class="panel medium" data-panel="blacklist">
        <header class="panel-header"><div class="label accent-rose">Blacklist</div><h2>Token / Dev Blocks</h2></header>
        <div id="blacklist" class="panel-body">Loading blacklist…</div>
      </article>
    </section>
  </main>

<script>
const $ = (id) => document.getElementById(id);
const pretty = (v) => JSON.stringify(v, null, 2);
const esc = (v) => String(v ?? '').replace(/[&<>'"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;',"'":'&#39;','"':'&quot;'}[c]));
const short = (v, n=64) => { const s = String(v ?? '—'); return s.length > n ? s.slice(0, n - 1) + '…' : s; };
const pill = (text, tone='') => `<span class="pill ${tone}">${esc(text)}</span>`;
const kv = (key, value) => `<div class="kv"><span>${esc(key)}</span><strong>${esc(value ?? '—')}</strong></div>`;
const empty = (text) => `<div class="empty">${esc(text)}</div>`;
function setPanel(id, html) { $(id).innerHTML = html; }
function log(msg, tone='') {
  const el = $('cycle-log');
  const color = tone === 'bad' ? ' style="color:#fecdd3"' : tone === 'ok' ? ' style="color:#bbf7d0"' : '';
  el.insertAdjacentHTML('beforeend', `<div class="log-line"${color}>${new Date().toLocaleTimeString()} ${esc(msg)}</div>`);
  el.scrollTop = el.scrollHeight;
}
async function api(path, options={}) {
  try {
    const res = await fetch(path, options);
    const text = await res.text();
    let data;
    try { data = text ? JSON.parse(text) : {}; } catch { data = { raw: text }; }
    if (!res.ok) return { success: false, command: path, error: data.error || res.statusText || text };
    return data;
  } catch (error) {
    return { success: false, command: path, error: String(error) };
  }
}
function unwrap(payload) { return payload && payload.data ? payload.data : {}; }
function renderError(payload) { return `<div class="empty">${esc(payload.error || 'Request failed')}</div>`; }
function renderStatus(payload) {
  if (!payload.success) return renderError(payload);
  const s = unwrap(payload);
  $('metric-runtime').textContent = s.status || 'running';
  $('metric-mode').textContent = s.dry_run ? 'DRY RUN' : 'LIVE';
  $('metric-positions').textContent = s.active_positions ?? 0;
  const schedule = s.schedule || {};
  return [
    kv('Status', s.status || 'running'),
    kv('Mode', s.dry_run ? 'Dry run guard active' : 'Live execution enabled'),
    kv('Active positions', s.active_positions ?? 0),
    kv('Screen every', `${schedule.screeningIntervalMin ?? '—'} min`),
    kv('Manage every', `${schedule.managementIntervalMin ?? '—'} min`),
    kv('PnL poll', `${schedule.pnlPollIntervalSecs ?? '—'} sec`),
    kv('State path', short(s.state_path, 72)),
  ].join('');
}
function renderPositions(payload) {
  if (!payload.success) return renderError(payload);
  const data = unwrap(payload);
  const positions = data.positions || [];
  if (!positions.length) return empty('No active positions in local state.');
  return `<div class="list">${positions.map(p => `<div class="item"><div class="item-title"><span>${esc(p.base_symbol || p.id || 'position')}</span>${pill(p.status || 'active', 'ok')}</div><div class="item-sub">${esc(short(p.id || p.position || '', 72))}</div><div class="item-sub">Pool: ${esc(short(p.pool_address || '', 72))}</div><div class="item-sub">Amount: ${esc(p.amount_sol ?? '—')} SOL · Range: ${esc(p.lower_bin ?? '—')} → ${esc(p.upper_bin ?? '—')}</div></div>`).join('')}</div>`;
}
function renderCandidates(payload) {
  if (!payload.success) return renderError(payload);
  const data = unwrap(payload);
  const candidates = data.candidates || [];
  const filtered = data.filtered_examples || [];
  $('metric-candidates').textContent = candidates.length;
  if (!candidates.length) {
    const examples = filtered.slice(0, 3).map(x => `<div class="item"><div class="item-title"><span>${esc(x.name || x.symbol || 'filtered')}</span>${pill('rejected','warn')}</div><div class="item-sub">${esc(x.reason || 'No reason provided')}</div></div>`).join('');
    return empty(`No deploy candidates. Screened ${data.total_screened ?? 0} pools.`) + (examples ? `<div class="list" style="margin-top:12px">${examples}</div>` : '');
  }
  return `<div class="list">${candidates.map(c => `<div class="item"><div class="item-title"><span>${esc(c.name || c.symbol || c.pool_address || 'candidate')}</span>${pill(c.score ?? 'candidate', 'ok')}</div><div class="item-sub">${esc(short(c.pool_address || c.address || '', 72))}</div><div class="item-sub">TVL ${esc(c.tvl ?? '—')} · Fees ${esc(c.fees_sol ?? c.total_fees_sol ?? '—')} SOL</div></div>`).join('')}</div>`;
}
function renderDecisions(payload) {
  if (!payload.success) return renderError(payload);
  const data = unwrap(payload);
  const decisions = data.decisions || [];
  $('metric-decisions') && ($('metric-decisions').textContent = decisions.length);
  if (!decisions.length) return empty('No decision-log entries yet.');
  return `<div class="list">${decisions.slice(0, 8).map(d => `<div class="item"><div class="item-title"><span>${esc(d.tool || d.action || 'decision')}</span>${pill(d.success === false ? 'failed' : 'ok', d.success === false ? 'bad' : 'ok')}</div><div class="item-sub">${esc(d.timestamp || '')}</div></div>`).join('')}</div>`;
}
function renderConfig(payload) {
  if (!payload.success) return renderError(payload);
  const c = unwrap(payload);
  const management = c.management || {};
  const risk = c.risk || {};
  const screening = c.screening || {};
  return [
    kv('Dry run', c.dryRun),
    kv('Deploy amount', `${management.deployAmountSol ?? '—'} SOL`),
    kv('Max positions', risk.maxPositions ?? '—'),
    kv('Screening tf', screening.timeframe || '—'),
    kv('Min TVL', screening.minTvl ?? '—'),
    kv('Config path', short(payload.path || '', 72)),
  ].join('');
}
function renderLessons(payload) {
  if (!payload.success) return renderError(payload);
  const data = unwrap(payload);
  const lessons = data.lessons || [];
  if (!lessons.length) return empty('No lessons recorded yet.');
  return `<div class="list">${lessons.slice(0, 4).map(l => `<div class="item"><div class="item-title"><span>${esc(l.role || 'lesson')}</span>${pill(Number(l.confidence ?? 0).toFixed(2))}</div><div class="item-sub">${esc(short(l.content || l.text || '', 120))}</div></div>`).join('')}</div>`;
}
function renderPerformance(payload) {
  if (!payload.success) return renderError(payload);
  const h = unwrap(payload).history || {};
  return [kv('24h records', h.count ?? 0), kv('Total PnL', h.total_pnl_sol ?? 0), kv('Win rate', h.win_rate_pct ?? '—')].join('');
}
function renderBlacklist(payload) {
  if (!payload.success) return renderError(payload);
  const data = unwrap(payload);
  const tokens = data.tokens?.blacklist || [];
  const devs = data.blocked_devs?.blocked_devs || [];
  if (!tokens.length && !devs.length) return empty('No token or developer blocks.');
  return [kv('Blocked tokens', tokens.length), kv('Blocked devs', devs.length)].join('');
}
function renderBalance(payload) {
  if (!payload.success) return renderError(payload);
  const data = unwrap(payload);
  if (data.available === false) return empty(data.reason || 'Wallet required.');
  return `<div class="result">${esc(pretty(data))}</div>`;
}
async function refreshAll() {
  const refreshButton = $('refresh-btn');
  refreshButton.disabled = true;
  log('refreshing live state');
  const [status, positions, candidates, decisions, config, lessons, performance, blacklist] = await Promise.all([
    api('/api/status'), api('/api/positions'), api('/api/candidates?limit=5'), api('/api/decisions'), api('/api/config'), api('/api/lessons'), api('/api/performance'), api('/api/blacklist')
  ]);
  setPanel('status', renderStatus(status));
  setPanel('positions', renderPositions(positions));
  setPanel('candidates', renderCandidates(candidates));
  setPanel('decisions', renderDecisions(decisions));
  setPanel('config', renderConfig(config));
  setPanel('lessons', renderLessons(lessons));
  setPanel('performance', renderPerformance(performance));
  setPanel('blacklist', renderBlacklist(blacklist));
  log('refresh complete', 'ok');
  refreshButton.disabled = false;
}
async function loadBalance() {
  const wallet = encodeURIComponent($('wallet').value.trim());
  const data = await api('/api/balance?wallet=' + wallet);
  setPanel('balance', renderBalance(data));
}
async function runControl(action) {
  log('control: ' + action);
  const data = await api('/api/control', { method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify({ action, wallet_sol: 0 }) });
  $('control-result').textContent = pretty(data);
  log(data.success ? `${action} finished` : `${action} failed`, data.success ? 'ok' : 'bad');
  await refreshAll();
}
async function runManualControl() {
  const action = $('control-action').value.trim();
  if (action === 'screen' || action === 'manage') return runControl(action);
  const pool = $('control-pool').value.trim();
  const position_id = $('control-position').value.trim();
  const amount = Number($('control-amount').value || 0);
  const args = { pool_address: pool, pool, position_id, amount_sol: amount, dry_run: true, skip_swap: true };
  log('manual action: ' + action);
  const data = await api('/api/control', { method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify({ action, args }) });
  $('control-result').textContent = pretty(data);
  log(data.success ? `manual ${action} finished` : `manual ${action} failed`, data.success ? 'ok' : 'bad');
  await refreshAll();
}
async function patchConfig() {
  let raw = $('config-value').value;
  let value; try { value = JSON.parse(raw); } catch { value = raw; }
  const body = { path: $('config-path').value.trim(), value };
  log('patch config: ' + body.path);
  const data = await api('/api/config', { method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify(body) });
  $('config').innerHTML = data.success ? renderConfig({success:true, data:data.data.config, path:data.data.path}) : renderError(data);
  $('control-result').textContent = pretty(data);
  log(data.success ? 'config patch saved' : 'config patch failed', data.success ? 'ok' : 'bad');
}
function openConfigPatch(){ $('config-path').focus(); }
function clearLog(){ $('cycle-log').innerHTML = ''; }
refreshAll();
setInterval(refreshAll, 15000);
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
        assert!(html.contains("renderStatus"));
        assert!(html.contains("renderCandidates"));
        assert!(html.contains("Operator-ready dashboard"));
        assert!(!html.contains("cdn.tailwindcss.com"));
        assert!(!html.contains("textContent = pretty(status)"));
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

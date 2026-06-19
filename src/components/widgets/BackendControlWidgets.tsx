'use client';

import { useEffect, useState } from 'react';
import { Cpu, TerminalSquare } from 'lucide-react';
import { GlassCard } from '../ui/GlassCard';
import { cachedJson } from '../../lib/clientCache';

type ApiPayload<T = any> = { success?: boolean; data?: T; error?: string };

const api = async <T,>(path: string, init?: RequestInit): Promise<ApiPayload<T>> => {
  const response = await fetch(path, init);
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) return { success: false, error: payload?.error ?? response.statusText };
  return payload;
};

const Field = ({ label, value }: { label: string; value: unknown }) => (
  <div className="backend-kv">
    <span>{label}</span>
    <strong title={String(value ?? '-')}>{String(value ?? '-')}</strong>
  </div>
);

export const BackendStatusWidget = () => {
  const [status, setStatus] = useState<any>();

  useEffect(() => {
    let mounted = true;
    const load = async () => {
      const payload = await cachedJson<ApiPayload>('/api/meridian/status', 8_000).catch(() => undefined);
      if (mounted) setStatus(payload?.data);
    };
    load();
    const timer = window.setInterval(load, 10_000);
    return () => { mounted = false; window.clearInterval(timer); };
  }, []);

  return (
    <GlassCard className="backend-card backend-status-card">
      <div className="terminal-title"><Cpu size={18} />BACKEND STATUS</div>
      <div className="terminal-divider" />
      <div className="backend-status-strip">
        <b>{status?.status ?? 'loading'}</b>
        <span>{status?.dry_run ? 'DRY RUN' : 'LIVE'}</span>
      </div>
      <div className="backend-grid-two">
        <Field label="Active positions" value={status?.active_positions ?? 0} />
        <Field label="Screen every" value={`${status?.schedule?.screeningIntervalMin ?? '-'} min`} />
        <Field label="Manage every" value={`${status?.schedule?.managementIntervalMin ?? '-'} min`} />
        <Field label="PnL poll" value={`${status?.schedule?.pnlPollIntervalSecs ?? '-'} sec`} />
        <Field label="State" value={status?.state_path ? 'connected' : 'not set'} />
        <Field label="Data dir" value={status?.data_dir ? 'available' : 'unknown'} />
      </div>
    </GlassCard>
  );
};

export const BackendControlsWidget = () => {
  const [action, setAction] = useState('screen');
  const [pool, setPool] = useState('');
  const [positionId, setPositionId] = useState('');
  const [amount, setAmount] = useState('0.10');
  const [result, setResult] = useState('No action yet.');
  const [busy, setBusy] = useState(false);

  const run = async () => {
    setBusy(true);
    const body = action === 'screen' || action === 'manage'
      ? { action, wallet_sol: 0 }
      : { action, args: { pool, pool_address: pool, position_id: positionId, amount_sol: Number(amount || 0), dry_run: true, skip_swap: true } };
    const payload = await api('/api/meridian/control', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
    setResult(JSON.stringify(payload, null, 2));
    setBusy(false);
  };

  return (
    <GlassCard className="backend-card backend-controls-card">
      <div className="terminal-title"><TerminalSquare size={18} />MANUAL CONTROLS</div>
      <div className="terminal-divider" />
      <p className="backend-note">All actions go through <code>/api/meridian/control</code>. Dry-run guard stays active from backend config.</p>
      <div className="backend-form-grid">
        <label>Action<select value={action} onChange={(event) => setAction(event.target.value)}><option>screen</option><option>manage</option><option>deploy_position</option><option>claim_fees</option><option>close_position</option><option>swap_token</option></select></label>
        <label>Amount SOL<input value={amount} onChange={(event) => setAmount(event.target.value)} /></label>
        <label>Pool<input value={pool} onChange={(event) => setPool(event.target.value)} placeholder="pool address" /></label>
        <label>Position<input value={positionId} onChange={(event) => setPositionId(event.target.value)} placeholder="position id" /></label>
      </div>
      <button className="backend-button" type="button" disabled={busy} onClick={run}>{busy ? 'Executing...' : 'Execute Control'}</button>
      <pre className="backend-result">{result}</pre>
    </GlassCard>
  );
};



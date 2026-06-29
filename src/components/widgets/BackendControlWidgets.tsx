'use client';

import { useEffect, useState } from 'react';
import { Cpu, Power, Play, Square, RotateCw } from 'lucide-react';
import { GlassCard } from '../ui/GlassCard';
import { cachedJson } from '../../lib/clientCache';

type ApiPayload<T = any> = { success?: boolean; data?: T; error?: string };

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

// Admin-only start/stop of the trading agent (pm2 process meridian-backend).
// The frontend/dashboard and tunnel stay up regardless. Gated by middleware.
export const AgentControlWidget = () => {
  const [status, setStatus] = useState<string>('…');
  const [busy, setBusy] = useState(false);

  const load = async () => {
    try {
      const res = await fetch('/api/agent/control', { cache: 'no-store' });
      const data = await res.json();
      setStatus(data?.status ?? 'unknown');
    } catch {
      setStatus('unknown');
    }
  };

  useEffect(() => {
    load();
    const timer = window.setInterval(load, 8_000);
    return () => window.clearInterval(timer);
  }, []);

  const act = async (action: 'start' | 'stop' | 'restart') => {
    if (busy) return;
    if (action === 'stop' && !window.confirm('Stop the trading agent? It will stop screening and managing positions until you start it again.')) return;
    setBusy(true);
    try {
      const res = await fetch('/api/agent/control', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ action }),
      });
      const data = await res.json();
      setStatus(data?.status ?? 'unknown');
    } catch {
      /* status refresh on next poll */
    } finally {
      setBusy(false);
    }
  };

  const online = status === 'online';
  const label = online ? 'RUNNING' : status === 'stopped' ? 'STOPPED' : status.toUpperCase();

  return (
    <GlassCard className="backend-card agent-control-card">
      <div className="terminal-title"><Power size={18} />AGENT CONTROL</div>
      <div className="terminal-divider" />
      <div className="agent-state">
        <span className={`agent-dot ${online ? 'on' : 'off'}`} />
        <b>{label}</b>
        <span className="agent-sub">meridian-backend</span>
      </div>
      <div className="agent-actions">
        <button type="button" className="agent-btn start" disabled={busy || online} onClick={() => act('start')}><Play size={14} />Start</button>
        <button type="button" className="agent-btn stop" disabled={busy || !online} onClick={() => act('stop')}><Square size={14} />Stop</button>
        <button type="button" className="agent-btn restart" disabled={busy} onClick={() => act('restart')}><RotateCw size={14} />Restart</button>
      </div>
      <p className="backend-note">Frontend &amp; dashboard stay online — only the trading agent starts/stops.</p>
    </GlassCard>
  );
};




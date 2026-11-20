import { useEffect, useState } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import {
  listNetworkInterfaces,
  openFolder,
  pickDirectory,
  setDownloadsDir,
  setPreferredBindIp,
  type NetworkInterface,
} from '../lib/ipc';
import { BrandLockup } from '../components/Icons';
import type { Settings, ThemePreference } from '../types';

interface SettingsViewProps {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => void;
  networkError?: string;
  lanIp?: string;
  interfaceName?: string;
  /** Re-read LAN + firewall status after the bind interface changes. */
  onNetworkRefresh?: () => void;
}

const THEMES: Array<{ id: ThemePreference; label: string }> = [
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
  { id: 'system', label: 'System' },
];

function Row({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <div
      className="flex items-center justify-between gap-6 px-4 py-4"
      style={{ borderTop: '1px solid var(--border)' }}
    >
      <div className="min-w-0">
        <div className="text-[13.5px] font-medium" style={{ color: 'var(--text)' }}>
          {title}
        </div>
        <div className="mt-0.5 text-[12.5px]" style={{ color: 'var(--text-secondary)' }}>
          {description}
        </div>
      </div>
      <div className="flex flex-none items-center gap-3">{children}</div>
    </div>
  );
}

export function SettingsView({
  settings,
  onChange,
  networkError,
  lanIp,
  interfaceName,
  onNetworkRefresh,
}: SettingsViewProps) {
  const [appVersion, setAppVersion] = useState('');
  const [downloadsMessage, setDownloadsMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const [picking, setPicking] = useState(false);
  const [portDraft, setPortDraft] = useState(String(settings.port || ''));
  const [portMessage, setPortMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const [interfaces, setInterfaces] = useState<NetworkInterface[]>([]);
  const [bindMessage, setBindMessage] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    getVersion()
      .then(setAppVersion)
      .catch(() => setAppVersion('0.1.2'));
  }, []);

  useEffect(() => {
    setPortDraft(String(settings.port || ''));
  }, [settings.port]);

  useEffect(() => {
    let alive = true;
    listNetworkInterfaces()
      .then((list) => {
        if (alive) setInterfaces(list);
      })
      .catch(() => {
        if (alive) setInterfaces([]);
      });
    return () => {
      alive = false;
    };
  }, []);

  // A saved choice can outlive the network it was made on; keep showing it
  // instead of silently displaying a different adapter.
  const missingChoice = settings.bindIp !== '' && !interfaces.some((i) => i.ip === settings.bindIp);

  const chooseDownloads = async () => {
    setDownloadsMessage(null);
    setPicking(true);
    try {
      const picked = await pickDirectory('Choose a folder for received files');
      if (!picked) return;
      const effective = await setDownloadsDir(picked);
      onChange({ downloadsDir: effective });
      setDownloadsMessage({ ok: true, text: 'Saved. New sessions write there.' });
    } catch (error) {
      setDownloadsMessage({ ok: false, text: String(error) });
    } finally {
      setPicking(false);
    }
  };

  const resetDownloads = async () => {
    setDownloadsMessage(null);
    setPicking(true);
    try {
      const effective = await setDownloadsDir(null);
      onChange({ downloadsDir: effective });
      setDownloadsMessage({ ok: true, text: 'Back to your system Downloads folder.' });
    } catch (error) {
      setDownloadsMessage({ ok: false, text: String(error) });
    } finally {
      setPicking(false);
    }
  };

  const applyInterface = async (ip: string) => {
    setBindMessage(null);
    try {
      await setPreferredBindIp(ip || null);
      onChange({ bindIp: ip });
      setBindMessage({
        ok: true,
        text: ip ? `New sessions will bind to ${ip}.` : 'New sessions will use automatic detection.',
      });
      onNetworkRefresh?.();
    } catch (error) {
      setBindMessage({ ok: false, text: String(error) });
    }
  };

  const applyPort = () => {
    const trimmed = portDraft.trim();
    if (trimmed === '' || trimmed === '0') {
      onChange({ port: 0 });
      setPortMessage({ ok: true, text: 'Port set to automatic.' });
      return;
    }
    const port = Number.parseInt(trimmed, 10);
    if (!Number.isInteger(port) || port < 1024 || port > 65535) {
      setPortMessage({ ok: false, text: 'Enter a port between 1024 and 65535, or leave empty for automatic.' });
      return;
    }
    onChange({ port });
    setPortMessage({ ok: true, text: `Port ${port} will be used for the next transfer.` });
  };

  return (
    <div className="view-enter flex max-w-[640px] flex-col gap-4">
      <div>
        <h1 className="page-title">
          Settings
        </h1>
        <p className="page-sub">
          Preferences are saved on this device.
        </p>
      </div>

      <div className="card overflow-hidden">
        <div className="px-4 py-4">
          <div className="text-[13.5px] font-medium" style={{ color: 'var(--text)' }}>
            Appearance
          </div>
          <div className="mt-0.5 text-[12.5px]" style={{ color: 'var(--text-secondary)' }}>
            Choose how AerLynk looks.
          </div>
        </div>
        <div className="px-4 pb-4">
          <div className="segmented" role="radiogroup" aria-label="Theme">
            {THEMES.map((t) => (
              <button
                key={t.id}
                type="button"
                className={settings.theme === t.id ? 'active' : ''}
                onClick={() => onChange({ theme: t.id })}
              >
                {t.label}
              </button>
            ))}
          </div>
        </div>

        <Row
          title="Keep window on top"
          description="Pin AerLynk above other windows while transferring."
        >
          <label className="switch">
            <input
              type="checkbox"
              checked={settings.alwaysOnTop}
              onChange={(e) => onChange({ alwaysOnTop: e.target.checked })}
            />
            <span className="track" />
          </label>
        </Row>

        <Row
          title="Transfer port"
          description="Fixed port for phone connections. Empty = automatic (recommended)."
        >
          <input
            className="input mono w-[110px]"
            inputMode="numeric"
            placeholder="Auto"
            value={portDraft}
            onChange={(e) => {
              setPortDraft(e.target.value.replace(/[^0-9]/g, ''));
              setPortMessage(null);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') applyPort();
            }}
            aria-label="Preferred transfer port"
          />
          <button type="button" className="btn btn-secondary btn-sm" onClick={applyPort}>
            Apply
          </button>
        </Row>
        {portMessage && (
          <div
            className="px-4 pb-4 text-[12.5px]"
            style={{ color: portMessage.ok ? 'var(--green)' : 'var(--red)' }}
            role="status"
          >
            {portMessage.text}
          </div>
        )}

        <Row
          title="Network interface"
          description="Which adapter phones connect to. Automatic picks the one on the same network — pin it if you use a hotspot or several adapters."
        >
          <select
            className="input w-[220px]"
            value={settings.bindIp}
            onChange={(e) => void applyInterface(e.target.value)}
            aria-label="Network interface"
          >
            <option value="">Auto-detect</option>
            {missingChoice && <option value={settings.bindIp}>{settings.bindIp} (not available)</option>}
            {interfaces.map((i) => (
              <option key={i.ip} value={i.ip}>
                {i.name} — {i.ip}
                {i.isVirtual ? ' (virtual)' : ''}
              </option>
            ))}
          </select>
        </Row>
        {bindMessage && (
          <div
            className="px-4 pb-4 text-[12.5px]"
            style={{ color: bindMessage.ok ? 'var(--green)' : 'var(--red)' }}
            role="status"
          >
            {bindMessage.text}
          </div>
        )}
      </div>

      <div className="card overflow-hidden">
        <Row
          title="Downloads folder"
          description="Where received files are saved. Applies to the next session."
        >
          <span
            className="mono max-w-[240px] truncate text-[12px]"
            style={{ color: 'var(--text-secondary)' }}
            title={settings.downloadsDir}
          >
            {settings.downloadsDir || '…'}
          </span>
        </Row>
        <div className="flex flex-wrap items-center gap-2 px-4 pb-4">
          <button
            type="button"
            className="btn btn-secondary btn-sm"
            onClick={() => void chooseDownloads()}
            disabled={picking}
          >
            Change folder…
          </button>
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => void resetDownloads()}
            disabled={picking}
          >
            Use default
          </button>
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => {
              if (settings.downloadsDir) {
                void openFolder(settings.downloadsDir).catch(() => {});
              }
            }}
            disabled={!settings.downloadsDir}
          >
            Open
          </button>
        </div>
        {downloadsMessage && (
          <div
            className="px-4 pb-4 text-[12.5px]"
            style={{ color: downloadsMessage.ok ? 'var(--green)' : 'var(--red)' }}
            role="status"
          >
            {downloadsMessage.text}
          </div>
        )}
        <Row
          title="Network"
          description={
            networkError
              ? `Could not read network info: ${networkError}`
              : `Sharing over ${interfaceName ?? 'your network'}${lanIp ? ` (${lanIp})` : ''}. The port only opens during a transfer.`
          }
        >
          <span className="chip chip-green">Local only</span>
        </Row>
      </div>

      {/* About — the logotype, centred on its own printed plate. */}
      <div className="card flex flex-col items-center gap-3 px-5 py-6 text-center">
        <BrandLockup className="max-w-[300px]" />
        <div className="flex items-center gap-2">
          <span className="text-[14.5px] font-semibold" style={{ color: 'var(--text)' }}>
            AerLynk
          </span>
          {appVersion && <span className="chip chip-neutral">v{appVersion}</span>}
        </div>
        <div className="max-w-[360px] text-[12.5px]" style={{ color: 'var(--text-secondary)' }}>
          End-to-end encrypted local transfers. Keys never leave your devices.
        </div>
        <div
          className="mt-1 text-[10.5px] font-medium uppercase"
          style={{ color: 'var(--text-muted)', letterSpacing: '0.14em' }}
        >
          Crafted by <span style={{ color: 'var(--text)' }}>NACEREDDINE</span>
        </div>
      </div>
    </div>
  );
}

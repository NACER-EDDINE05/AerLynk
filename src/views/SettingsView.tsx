import { useEffect, useState } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import { getDownloadsDir } from '../lib/ipc';
import { LogoIcon } from '../components/Icons';
import type { Settings, ThemePreference } from '../types';

interface SettingsViewProps {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => void;
  networkError?: string;
  lanIp?: string;
  interfaceName?: string;
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

export function SettingsView({ settings, onChange, networkError, lanIp, interfaceName }: SettingsViewProps) {
  const [downloadsDir, setDownloadsDir] = useState('');
  const [appVersion, setAppVersion] = useState('');
  const [portDraft, setPortDraft] = useState(String(settings.port || ''));
  const [portMessage, setPortMessage] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    getDownloadsDir()
      .then(setDownloadsDir)
      .catch(() => {});
    getVersion()
      .then(setAppVersion)
      .catch(() => setAppVersion('0.1.1'));
  }, []);

  useEffect(() => {
    setPortDraft(String(settings.port || ''));
  }, [settings.port]);

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
      </div>

      <div className="card overflow-hidden">
        <Row
          title="Downloads folder"
          description="Where received files are saved."
        >
          <span className="mono max-w-[280px] truncate text-[12px]" style={{ color: 'var(--text-secondary)' }} title={downloadsDir}>
            {downloadsDir || '…'}
          </span>
        </Row>
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

      {/* About — who made this thing. */}
      <div className="card flex items-center gap-4 p-4">
        <span
          className="flex h-12 w-12 flex-none items-center justify-center rounded-xl"
          style={{ background: 'var(--surface-3)' }}
        >
          <LogoIcon size={28} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-[14.5px] font-semibold" style={{ color: 'var(--text)' }}>
              AerLynk
            </span>
            {appVersion && <span className="chip chip-neutral">v{appVersion}</span>}
          </div>
          <div className="mt-0.5 text-[12.5px]" style={{ color: 'var(--text-secondary)' }}>
            End-to-end encrypted local transfers. Keys never leave your devices.
          </div>
        </div>
        <div className="flex-none text-right">
          <div
            className="text-[10.5px] font-medium uppercase"
            style={{ color: 'var(--text-muted)', letterSpacing: '0.12em' }}
          >
            Crafted by
          </div>
          <div
            className="text-[14px] font-bold"
            style={{
              letterSpacing: '0.04em',
              background: 'linear-gradient(120deg, var(--accent), var(--accent-2))',
              WebkitBackgroundClip: 'text',
              backgroundClip: 'text',
              color: 'transparent',
            }}
          >
            NACEREDDINE
          </div>
        </div>
      </div>
    </div>
  );
}

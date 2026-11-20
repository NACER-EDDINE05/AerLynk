import { useCallback, useEffect, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { setDownloadsDir, setPreferredBindIp, setPreferredPort } from '../lib/ipc';
import type { Settings, ThemePreference } from '../types';

const STORAGE_KEY = 'aerlynk.settings';

const DEFAULTS: Settings = {
  theme: 'system',
  alwaysOnTop: false,
  port: 0,
  bindIp: '',
  downloadsDir: '',
};

function load(): Settings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<Settings>;
    return {
      theme: parsed.theme ?? DEFAULTS.theme,
      alwaysOnTop: parsed.alwaysOnTop ?? DEFAULTS.alwaysOnTop,
      port: typeof parsed.port === 'number' ? parsed.port : DEFAULTS.port,
      bindIp: typeof parsed.bindIp === 'string' ? parsed.bindIp : DEFAULTS.bindIp,
      downloadsDir: typeof parsed.downloadsDir === 'string' ? parsed.downloadsDir : DEFAULTS.downloadsDir,
    };
  } catch {
    return DEFAULTS;
  }
}

/** True when running inside the Tauri webview (vs. a plain browser tab). */
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export function useSettings(): [Settings, (patch: Partial<Settings>) => void] {
  const [settings, setSettings] = useState<Settings>(load);
  const appliedPort = useRef<number | null>(null);
  const appliedBindIp = useRef<string | null>(null);
  const appliedDownloadsDir = useRef<string | null>(null);

  const update = useCallback((patch: Partial<Settings>) => {
    setSettings((prev) => {
      const next = { ...prev, ...patch };
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      } catch {
        /* storage unavailable — settings live for this session only */
      }
      return next;
    });
  }, []);

  // Theme: reflect the preference onto <html data-theme>.
  useEffect(() => {
    const root = document.documentElement;
    const apply = () => {
      const dark =
        settings.theme === 'dark' ||
        (settings.theme === 'system' &&
          window.matchMedia('(prefers-color-scheme: dark)').matches);
      root.dataset.theme = dark ? 'dark' : 'light';
    };
    apply();
    if (settings.theme !== 'system') return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    mq.addEventListener('change', apply);
    return () => mq.removeEventListener('change', apply);
  }, [settings.theme]);

  // Always on top.
  useEffect(() => {
    if (!isTauri) return;
    getCurrentWindow()
      .setAlwaysOnTop(settings.alwaysOnTop)
      .catch(() => {});
  }, [settings.alwaysOnTop]);

  // Preferred port — push to the backend once per change (0 = automatic).
  useEffect(() => {
    if (!isTauri || appliedPort.current === settings.port) return;
    appliedPort.current = settings.port;
    setPreferredPort(settings.port).catch(() => {});
  }, [settings.port]);

  // Preferred LAN interface — restored once at startup so a saved choice
  // survives restarts. Changes made in Settings are applied there, where the
  // backend's validation error can be shown inline.
  useEffect(() => {
    if (!isTauri || appliedBindIp.current !== null) return;
    appliedBindIp.current = settings.bindIp;
    setPreferredBindIp(settings.bindIp || null).catch(() => {});
  }, [settings.bindIp]);

  // Downloads folder — adopt the backend's effective path once at startup (the
  // OS default when the user has not chosen one), so the UI always shows the
  // real folder rather than an empty setting.
  useEffect(() => {
    if (!isTauri || appliedDownloadsDir.current !== null) return;
    appliedDownloadsDir.current = settings.downloadsDir;
    setDownloadsDir(settings.downloadsDir || null)
      .then((effective) => {
        if (effective && effective !== settings.downloadsDir) {
          update({ downloadsDir: effective });
        }
      })
      .catch(() => {});
  }, [settings.downloadsDir, update]);

  return [settings, update];
}

export type { ThemePreference };

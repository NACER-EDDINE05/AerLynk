import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn, Event } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';

export interface SessionFileInfo {
  id: number;
  originalName: string;
  size: number;
}

/** Shape returned by start_send_session / start_receive_session. */
export interface StartSessionResult {
  token: string;
  displayCode: string;
  /** Full session URL encoded into the QR (AES key lives in the fragment). */
  url: string;
  qrSvg: string;
  files: SessionFileInfo[];
}

export interface ProgressEvent {
  token: string;
  sentBytes: number;
  totalBytes: number;
  files: Array<{
    id: number;
    sentBytes: number;
    status: 'pending' | 'active' | 'done' | 'failed';
  }>;
  transferSpeed: number;
}

export interface SessionDeliveredData {
  token: string;
  /** Names of the delivered files (send sources / received uploads). */
  files?: string[];
  bytes?: number;
}

export interface SessionEndedData {
  token: string;
  cause?: string;
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

export const startSendSession = (paths: string[]): Promise<StartSessionResult> =>
  invoke<StartSessionResult>('start_send_session', { paths });

export const startReceiveSession = (): Promise<StartSessionResult> =>
  invoke<StartSessionResult>('start_receive_session');

export const endSession = (token: string): Promise<void> =>
  invoke('end_session', { token });

export const cancelTransfer = (token: string): Promise<void> =>
  invoke('cancel_transfer', { token });

// ---------------------------------------------------------------------------
// Files / folders
// ---------------------------------------------------------------------------

export const pickFiles = async (): Promise<string[]> => {
  const selected = await open({
    multiple: true,
    directory: false,
    title: 'Select files to send',
  });
  if (!selected) return [];
  return (Array.isArray(selected) ? selected : [selected]).filter(
    (p): p is string => typeof p === 'string'
  );
};

export const getDownloadsDir = (): Promise<string> =>
  invoke<string>('get_downloads_dir');

export const openFolder = (path: string): Promise<void> =>
  invoke('open_folder', { path });

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

export const setPreferredPort = (port: number): Promise<void> =>
  invoke('set_preferred_port', { port });

// ---------------------------------------------------------------------------
// Network / firewall
// ---------------------------------------------------------------------------

export interface NetworkInfoResult {
  lanIp: string;
  interfaceName: string;
  port: number;
}

export interface FirewallStatusResult {
  status: 'clean' | 'missingAllowRule' | 'staleBlockRules' | 'unknown';
  blocks?: string[];
}

export const getNetworkInfo = (): Promise<NetworkInfoResult> =>
  invoke('get_network_info');

export const checkFirewall = (): Promise<FirewallStatusResult> =>
  invoke('check_firewall');

export const repairFirewall = (): Promise<void> =>
  invoke('repair_firewall');

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

export const onProgress = (callback: (progress: ProgressEvent) => void): Promise<UnlistenFn> =>
  listen('transfer-progress', (event: Event<ProgressEvent>) => callback(event.payload));

export const onSessionDelivered = (
  callback: (data: SessionDeliveredData) => void
): Promise<UnlistenFn> =>
  listen('session-delivered', (event: Event<SessionDeliveredData>) => callback(event.payload));

export const onSessionCancelled = (
  callback: (data: SessionEndedData) => void
): Promise<UnlistenFn> =>
  listen('session-cancelled', (event: Event<SessionEndedData>) => callback(event.payload));

export const onSessionError = (
  callback: (data: SessionEndedData) => void
): Promise<UnlistenFn> =>
  listen('session-error', (event: Event<SessionEndedData>) => callback(event.payload));

export type View = 'send' | 'receive' | 'history' | 'settings';

export type TransferPhase = 'idle' | 'ready' | 'transferring' | 'done' | 'error';

export type FileStatus = 'pending' | 'active' | 'done' | 'failed';

export interface SessionFile {
  id: number;
  originalName: string;
  size: number;
  sentBytes: number;
  status: FileStatus;
}

export interface HistoryEntry {
  id: string;
  kind: 'send' | 'receive';
  fileNames: string[];
  totalBytes: number;
  status: 'completed' | 'cancelled' | 'failed';
  timestamp: number;
}

export type ThemePreference = 'light' | 'dark' | 'system';

export interface Settings {
  theme: ThemePreference;
  alwaysOnTop: boolean;
  /** Preferred phone-facing port; 0 = OS-assigned ephemeral. */
  port: number;
}

export type FirewallStatus = 'clean' | 'missingAllowRule' | 'staleBlockRules' | 'unknown';

export interface NetworkStatus {
  lanIp?: string;
  interfaceName?: string;
  port?: number;
  firewall: FirewallStatus;
  firewallBlocks?: string[];
  error?: string;
}

import { AlertIcon, DownloadIcon, HistoryIcon, LogoIcon, RefreshIcon, SettingsIcon, ShieldIcon, UploadIcon, WifiIcon } from './Icons';
import type { NetworkStatus, View } from '../types';

interface SidebarProps {
  active: View;
  onSelect: (view: View) => void;
  network: NetworkStatus;
  repairing: boolean;
  onRepair: () => void;
  onRefreshNetwork: () => void;
}

const NAV: Array<{ id: View; label: string; icon: typeof UploadIcon }> = [
  { id: 'send', label: 'Send', icon: UploadIcon },
  { id: 'receive', label: 'Receive', icon: DownloadIcon },
  { id: 'history', label: 'History', icon: HistoryIcon },
  { id: 'settings', label: 'Settings', icon: SettingsIcon },
];

export function Sidebar({ active, onSelect, network, repairing, onRepair, onRefreshNetwork }: SidebarProps) {
  const firewallBad = network.firewall === 'missingAllowRule' || network.firewall === 'staleBlockRules';
  const offline = !network.lanIp;

  return (
    <aside
      className="sidebar flex h-full w-[232px] shrink-0 flex-col border-r px-3 py-4"
      style={{ borderColor: 'var(--border)' }}
    >
      <div className="flex items-center gap-2.5 px-2 pb-5">
        <LogoIcon size={26} />
        <span className="text-[15px] font-semibold" style={{ color: 'var(--text)' }}>
          AerLynk
        </span>
      </div>

      <nav className="flex flex-col gap-0.5">
        {NAV.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            type="button"
            className={`nav-item ${active === id ? 'active' : ''}`}
            onClick={() => onSelect(id)}
          >
            <Icon size={16} />
            {label}
          </button>
        ))}
      </nav>

      <div className="mt-auto">
        {firewallBad ? (
          <div
            className="card-muted flex flex-col gap-2 p-3"
            style={{ background: 'var(--red-soft)', borderColor: 'transparent' }}
          >
            <div className="flex items-center gap-2 text-[12.5px] font-medium" style={{ color: 'var(--red)' }}>
              <AlertIcon size={14} />
              Firewall is blocking AerLynk
            </div>
            <p className="m-0 text-[11.5px]" style={{ color: 'var(--text-secondary)' }}>
              Phones on your network can&apos;t reach this PC until the firewall rule is added.
            </p>
            <button type="button" className="btn btn-danger btn-sm" onClick={onRepair} disabled={repairing}>
              <ShieldIcon size={13} />
              {repairing ? 'Repairing…' : 'Repair now'}
            </button>
          </div>
        ) : (
          <button
            type="button"
            className="card-muted flex w-full items-center gap-2.5 p-3 text-left"
            style={{ cursor: 'default' }}
            title={network.error ? `Network issue: ${network.error}` : `Connected via ${network.interfaceName ?? ''}`}
          >
            {offline ? (
              <WifiIcon size={15} style={{ color: 'var(--amber)' }} />
            ) : (
              <span style={{ color: 'var(--green)', display: 'inline-flex' }}>
                <WifiIcon size={15} />
              </span>
            )}
            <span className="min-w-0 flex-1">
              <span className="block text-[12.5px] font-medium" style={{ color: 'var(--text)' }}>
                {offline ? 'No network' : network.interfaceName || 'Network'}
              </span>
              <span className="mono block truncate text-[11px]" style={{ color: 'var(--text-muted)' }}>
                {network.lanIp ?? 'Check connection'}
              </span>
            </span>
            <span
              className="icon-btn"
              role="button"
              tabIndex={0}
              aria-label="Refresh network status"
              onClick={(e) => {
                e.stopPropagation();
                onRefreshNetwork();
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.stopPropagation();
                  onRefreshNetwork();
                }
              }}
            >
              <RefreshIcon size={13} />
            </span>
          </button>
        )}
      </div>
    </aside>
  );
}

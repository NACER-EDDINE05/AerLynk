import { useState } from 'react';
import { Sidebar } from './components/Sidebar';
import { SendView } from './views/SendView';
import { ReceiveView } from './views/ReceiveView';
import { HistoryView } from './views/HistoryView';
import { SettingsView } from './views/SettingsView';
import { useNetworkStatus } from './hooks/useNetworkStatus';
import { useHistory } from './hooks/useHistory';
import { useSettings } from './hooks/useSettings';
import { useSpotlight } from './hooks/useSpotlight';
import type { View } from './types';
import './index.css';

/** True when running inside the Tauri webview (vs. a plain browser tab). */
export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

function App() {
  const [view, setView] = useState<View>('send');
  const [settings, setSettings] = useSettings();
  const history = useHistory();
  const network = useNetworkStatus();
  const spotlightRef = useSpotlight<HTMLDivElement>();

  return (
    <div ref={spotlightRef} className="flex h-full">
      {/* Atmospheric backdrop: aurora glows, hairline grid, film grain. */}
      <div className="app-atmosphere" aria-hidden>
        <div className="aurora aurora-1" />
        <div className="aurora aurora-2" />
        <div className="grid-veil" />
        <div className="noise-veil" />
      </div>

      <Sidebar
        active={view}
        onSelect={setView}
        network={network.status}
        repairing={network.repairing}
        onRepair={() => void network.repair()}
        onRefreshNetwork={network.refresh}
      />

      <main className="relative z-[1] min-w-0 flex-1 overflow-y-auto">
        {/* Views stay mounted so a transfer keeps running if you switch tabs. */}
        <div className={view === 'send' ? 'block p-8' : 'hidden'}>
          <div className="mx-auto max-w-[620px]">
            <SendView addHistory={history.add} />
          </div>
        </div>
        <div className={view === 'receive' ? 'block p-8' : 'hidden'}>
          <div className="mx-auto max-w-[620px]">
            <ReceiveView addHistory={history.add} />
          </div>
        </div>
        <div className={view === 'history' ? 'block p-8' : 'hidden'}>
          <div className="mx-auto max-w-[760px]">
            <HistoryView entries={history.entries} onClear={history.clear} />
          </div>
        </div>
        <div className={view === 'settings' ? 'block p-8' : 'hidden'}>
          <div className="mx-auto max-w-[660px]">
            <SettingsView
              settings={settings}
              onChange={setSettings}
              networkError={network.status.error}
              lanIp={network.status.lanIp}
              interfaceName={network.status.interfaceName}
            />
          </div>
        </div>
      </main>
    </div>
  );
}

export default App;

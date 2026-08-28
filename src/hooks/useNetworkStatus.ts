import { useCallback, useEffect, useState } from 'react';
import {
  checkFirewall,
  getNetworkInfo,
  NetworkInfoResult,
  repairFirewall,
} from '../lib/ipc';
import type { NetworkStatus } from '../types';

/** Live LAN + firewall status for the sidebar indicator. */
export function useNetworkStatus(): {
  status: NetworkStatus;
  refresh: () => void;
  repairing: boolean;
  repair: () => Promise<void>;
} {
  const [status, setStatus] = useState<NetworkStatus>({ firewall: 'unknown' });
  const [repairing, setRepairing] = useState(false);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let cancelled = false;

    const run = async () => {
      const next: NetworkStatus = { firewall: 'unknown' };
      try {
        const info: NetworkInfoResult = await getNetworkInfo();
        next.lanIp = info.lanIp;
        next.interfaceName = info.interfaceName;
        next.port = info.port;
      } catch (error) {
        next.error = String(error);
      }
      try {
        const fw = await checkFirewall();
        next.firewall = fw.status;
        next.firewallBlocks = fw.blocks;
      } catch {
        next.firewall = 'unknown';
      }
      if (!cancelled) setStatus(next);
    };

    void run();
    return () => {
      cancelled = true;
    };
  }, [tick]);

  const refresh = useCallback(() => setTick((t) => t + 1), []);

  const repair = useCallback(async () => {
    setRepairing(true);
    try {
      await repairFirewall();
      // Give the firewall a moment, then re-check.
      await new Promise((r) => setTimeout(r, 800));
      refresh();
    } finally {
      setRepairing(false);
    }
  }, [refresh]);

  return { status, refresh, repairing, repair };
}

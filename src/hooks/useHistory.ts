import { useCallback, useState } from 'react';
import type { HistoryEntry } from '../types';

const STORAGE_KEY = 'airlynk.history';
const MAX_ENTRIES = 50;

function load(): HistoryEntry[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as HistoryEntry[]) : [];
  } catch {
    return [];
  }
}

export function useHistory() {
  const [entries, setEntries] = useState<HistoryEntry[]>(load);

  const persist = useCallback((next: HistoryEntry[]) => {
    setEntries(next);
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next.slice(0, MAX_ENTRIES)));
    } catch {
      /* storage unavailable — history lives for this session only */
    }
  }, []);

  const add = useCallback(
    (entry: Omit<HistoryEntry, 'id' | 'timestamp'>) => {
      persist([
        { ...entry, id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`, timestamp: Date.now() },
        ...load(),
      ].slice(0, MAX_ENTRIES));
    },
    [persist]
  );

  const clear = useCallback(() => persist([]), [persist]);

  return { entries, add, clear };
}

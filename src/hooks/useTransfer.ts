import { useCallback, useEffect, useRef, useState } from 'react';
import { UnlistenFn } from '@tauri-apps/api/event';
import {
  cancelTransfer,
  endSession,
  onProgress,
  onSessionCancelled,
  onSessionDelivered,
  onSessionError,
  ProgressEvent,
  startReceiveSession,
  startSendSession,
} from '../lib/ipc';
import type { SessionFile, TransferPhase } from '../types';

export interface TransferState {
  phase: TransferPhase;
  token: string;
  displayCode: string;
  url: string;
  qrSvg: string;
  files: SessionFile[];
  bytes: number;
  total: number;
  speed: number;
  error?: string;
}

const INITIAL: TransferState = {
  phase: 'idle',
  token: '',
  displayCode: '',
  url: '',
  qrSvg: '',
  files: [],
  bytes: 0,
  total: 0,
  speed: 0,
};

export interface TransferOutcome {
  ok: boolean;
  fileNames: string[];
  totalBytes: number;
  status: 'completed' | 'cancelled' | 'failed';
}

/**
 * Drives one phone-facing session (send or receive): start, live progress,
 * lifecycle events, cancel. `onFinished` fires exactly once per session.
 */
export function useTransfer(
  kind: 'send' | 'receive',
  onFinished: (outcome: TransferOutcome) => void
) {
  const [state, setState] = useState<TransferState>(INITIAL);
  const tokenRef = useRef('');
  const finishedRef = useRef(onFinished);
  finishedRef.current = onFinished;

  const finishOnce = useCallback(
    (ok: boolean, status: 'completed' | 'cancelled' | 'failed', error?: string) => {
      const token = tokenRef.current;
      if (!token) return;
      tokenRef.current = '';
      setState((s) => {
        if (s.phase === 'done') return s;
        const names = s.files.filter((f) => f.status === 'done').map((f) => f.originalName);
        finishedRef.current({
          ok,
          fileNames: names,
          totalBytes: s.bytes > 0 ? s.bytes : s.total,
          status,
        });
        return { ...s, phase: 'done', error };
      });
    },
    []
  );

  // Lifecycle + progress events (subscribe once; filter by the live token).
  useEffect(() => {
    const unsubs: Array<UnlistenFn | undefined> = [];
    let disposed = false;

    const setup = async () => {
      unsubs.push(
        await onProgress((p: ProgressEvent) => {
          if (p.token !== tokenRef.current) return;
          setState((s) => ({
            ...s,
            phase: 'transferring',
            bytes: p.sentBytes,
            total: p.totalBytes || s.total,
            speed: p.transferSpeed,
            files: s.files.length
              ? s.files.map((f) => {
                  const pf = p.files.find((x) => x.id === f.id);
                  return pf ? { ...f, sentBytes: pf.sentBytes, status: pf.status } : f;
                })
              : p.files.map((pf) => ({
                  id: pf.id,
                  originalName: `Incoming file ${pf.id + 1}`,
                  size: 0,
                  sentBytes: pf.sentBytes,
                  status: pf.status,
                })),
          }));
        })
      );
      unsubs.push(
        await onSessionDelivered((data) => {
          if (data.token !== tokenRef.current) return;
          setState((s) => {
            const names = data.files?.length
              ? data.files
              : s.files.filter((f) => f.status === 'done').map((f) => f.originalName);
            const bytes = data.bytes ?? (s.bytes > 0 ? s.bytes : s.total);
            finishedRef.current({ ok: true, fileNames: names, totalBytes: bytes, status: 'completed' });
            tokenRef.current = '';
            return {
              ...s,
              phase: 'done',
              bytes: bytes,
              files: data.files?.length
                ? data.files.map((n, i) => ({
                    id: s.files[i]?.id ?? i,
                    originalName: n,
                    size: 0,
                    sentBytes: 0,
                    status: 'done' as const,
                  }))
                : s.files.map((f) => ({ ...f, status: 'done' as const })),
            };
          });
        })
      );
      unsubs.push(
        await onSessionCancelled((data) => {
          if (data.token !== tokenRef.current) return;
          finishOnce(false, 'cancelled');
        })
      );
      unsubs.push(
        await onSessionError((data) => {
          if (data.token !== tokenRef.current) return;
          finishOnce(false, 'failed', data.cause);
        })
      );
    };

    void setup().catch(() => {});
    return () => {
      disposed = true;
      void disposed;
      unsubs.forEach((u) => u?.());
    };
  }, [finishOnce]);

  const start = useCallback(
    async (paths?: string[]) => {
      // Close any previous session before opening a new one.
      if (tokenRef.current) {
        const prev = tokenRef.current;
        tokenRef.current = '';
        void endSession(prev).catch(() => {});
      }
      setState(INITIAL);
      try {
        const res =
          kind === 'send' ? await startSendSession(paths ?? []) : await startReceiveSession();
        tokenRef.current = res.token;
        setState({
          phase: 'ready',
          token: res.token,
          displayCode: res.displayCode,
          url: res.url,
          qrSvg: res.qrSvg,
          files: res.files.map((f) => ({ ...f, sentBytes: 0, status: 'pending' })),
          bytes: 0,
          total: res.files.reduce((acc, f) => acc + f.size, 0),
          speed: 0,
        });
      } catch (error) {
        setState({ ...INITIAL, phase: 'error', error: String(error) });
      }
    },
    [kind]
  );

  const reset = useCallback(async () => {
    if (tokenRef.current) {
      const token = tokenRef.current;
      tokenRef.current = '';
      void endSession(token).catch(() => {});
    }
    setState(INITIAL);
  }, []);

  const cancel = useCallback(async () => {
    if (tokenRef.current) {
      await cancelTransfer(tokenRef.current).catch(() => {});
    }
  }, []);

  return { state, start, reset, cancel };
}

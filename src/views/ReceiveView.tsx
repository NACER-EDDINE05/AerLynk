import { useCallback, useEffect, useState } from 'react';
import { FileList } from '../components/FileList';
import { QrCard } from '../components/QrCard';
import { CheckIcon, DownloadIcon, FolderIcon, XIcon } from '../components/Icons';
import { useTransfer, TransferOutcome } from '../hooks/useTransfer';
import { getDownloadsDir, openFolder } from '../lib/ipc';
import { formatBytes } from '../lib/format';
import type { HistoryEntry } from '../types';

type AddHistory = (entry: Omit<HistoryEntry, 'id' | 'timestamp'>) => void;

interface ReceiveViewProps {
  addHistory: AddHistory;
  /** Effective destination folder, resolved by the backend. */
  downloadsDir?: string;
}

export function ReceiveView({ addHistory, downloadsDir: chosenDir }: ReceiveViewProps) {
  const [fallbackDir, setFallbackDir] = useState('');
  const downloadsDir = chosenDir || fallbackDir;

  useEffect(() => {
    if (chosenDir) return;
    getDownloadsDir()
      .then(setFallbackDir)
      .catch(() => {});
  }, [chosenDir]);

  const onFinished = useCallback(
    (outcome: TransferOutcome) => {
      addHistory({
        kind: 'receive',
        fileNames: outcome.fileNames,
        totalBytes: outcome.totalBytes,
        status: outcome.status,
      });
    },
    [addHistory]
  );

  const { state, start, reset, cancel } = useTransfer('receive', onFinished);

  const active = state.phase === 'ready' || state.phase === 'transferring';
  const received = state.files.filter((f) => f.status === 'done');

  const openDownloads = () => {
    if (downloadsDir) void openFolder(downloadsDir).catch((err) => console.error('Failed to open Downloads folder:', err));
  };

  // ---- Live session --------------------------------------------------------
  if (active || state.phase === 'done') {
    const pct = state.total > 0 ? Math.round((state.bytes / state.total) * 100) : 0;
    return (
      <div className="view-enter flex flex-col gap-4">
        {state.phase === 'done' ? (
          <div className="card flex flex-col items-center gap-3 p-10 text-center">
            <span className="seal">
              <CheckIcon size={26} />
            </span>
            <div>
              <div className="text-[16px] font-semibold" style={{ color: 'var(--text)' }}>
                {received.length} {received.length === 1 ? 'file' : 'files'} received
              </div>
              <div className="mt-1 text-[13px]" style={{ color: 'var(--text-secondary)' }}>
                {formatBytes(state.bytes || state.total)} saved to your Downloads folder
              </div>
            </div>
            {received.length > 0 && (
              <div className="w-full max-w-[420px] text-left">
                <FileList files={received} />
              </div>
            )}
            <div className="mt-2 flex gap-2">
              <button type="button" className="btn btn-secondary" onClick={() => void openDownloads()}>
                <FolderIcon size={15} />
                Open folder
              </button>
              <button type="button" className="btn btn-primary" onClick={() => void reset()}>
                Receive more
              </button>
            </div>
          </div>
        ) : (
          <>
            <div className="card">
              <QrCard
                qrSvg={state.qrSvg}
                displayCode={state.displayCode}
                url={state.url}
                hint={state.phase === 'ready' ? 'Waiting for your phone — scan to upload' : 'Receiving…'}
                pulse={state.phase === 'ready'}
                footer={
                  state.phase === 'transferring' ? (
                    <div className="flex items-center gap-3">
                      <div className="progress-track flex-1">
                        <div className="progress-fill" style={{ width: `${pct}%` }} />
                      </div>
                      <span className="flex-none text-[12px]" style={{ color: 'var(--text-secondary)' }}>
                        {formatBytes(state.bytes)}
                        {state.total > 0 && ` / ${formatBytes(state.total)}`}
                      </span>
                    </div>
                  ) : undefined
                }
              />
            </div>
            {state.files.length > 0 && (
              <FileList files={state.files} live={state.phase === 'transferring'} />
            )}
            <div className="flex items-center justify-between">
              <button type="button" className="btn btn-ghost" onClick={() => void reset()}>
                <XIcon size={14} />
                Discard session
              </button>
              {state.phase === 'ready' && (
                <button type="button" className="btn btn-secondary" onClick={() => void cancel()}>
                  Cancel
                </button>
              )}
            </div>
          </>
        )}
      </div>
    );
  }

  // ---- Idle ------------------------------------------------------------------
  return (
    <div className="view-enter flex flex-col gap-4">
      <div>
        <h1 className="page-title">
          Receive files
        </h1>
        <p className="page-sub">
          Let a phone push files to this PC over your local network.
        </p>
      </div>

      <div className="card flex flex-col items-center gap-4 p-10 text-center">
        <span
          className="flex h-14 w-14 items-center justify-center rounded-full"
          style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}
        >
          <DownloadIcon size={26} />
        </span>
        <div className="max-w-[380px]">
          <div className="text-[15px] font-medium" style={{ color: 'var(--text)' }}>
            A QR code opens a private upload page on your phone
          </div>
          <p className="mb-0 mt-2 text-[13px]" style={{ color: 'var(--text-secondary)' }}>
            Files are encrypted end-to-end and land directly in your Downloads folder.
            The server only runs while a session is live.
          </p>
        </div>
        <button type="button" className="btn btn-primary btn-lg" onClick={() => void start()}>
          <DownloadIcon size={16} />
          Receive files
        </button>
      </div>

      {state.phase === 'error' && (
        <div
          className="card-muted flex items-center gap-2 p-3 text-[13px]"
          style={{ background: 'var(--red-soft)', borderColor: 'transparent', color: 'var(--red)' }}
        >
          {state.error ?? 'Could not start the session.'}
        </div>
      )}

      {downloadsDir && (
        <div className="text-[12px]" style={{ color: 'var(--text-muted)' }}>
          Received files are saved to <span className="mono select-text">{downloadsDir}</span>
        </div>
      )}
    </div>
  );
}

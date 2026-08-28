import { useCallback, useState } from 'react';
import { Dropzone } from '../components/Dropzone';
import { FileList } from '../components/FileList';
import { QrCard } from '../components/QrCard';
import { CheckIcon, UploadIcon, XIcon } from '../components/Icons';
import { useTransfer, TransferOutcome } from '../hooks/useTransfer';
import { formatBytes, formatSpeed } from '../lib/format';
import type { HistoryEntry } from '../types';

type AddHistory = (entry: Omit<HistoryEntry, 'id' | 'timestamp'>) => void;

interface SendViewProps {
  addHistory: AddHistory;
}

export function SendView({ addHistory }: SendViewProps) {
  const [staged, setStaged] = useState<string[]>([]);

  const onFinished = useCallback(
    (outcome: TransferOutcome) => {
      addHistory({
        kind: 'send',
        fileNames: outcome.fileNames,
        totalBytes: outcome.totalBytes,
        status: outcome.status,
      });
    },
    [addHistory]
  );

  const { state, start, reset, cancel } = useTransfer('send', onFinished);

  const active = state.phase === 'ready' || state.phase === 'transferring';

  const begin = () => {
    const real = staged.filter((p) => !p.startsWith('browser:'));
    if (real.length === 0) return;
    void start(real);
  };

  const backToStaging = async () => {
    await reset();
    setStaged([]);
  };

  // ---- Live session: QR + progress --------------------------------------
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
                Transfer complete
              </div>
              <div className="mt-1 text-[13px]" style={{ color: 'var(--text-secondary)' }}>
                {state.files.filter((f) => f.status === 'done').length}{' '}
                {state.files.filter((f) => f.status === 'done').length === 1 ? 'file' : 'files'} ·{' '}
                {formatBytes(state.bytes || state.total)}
              </div>
            </div>
            <button type="button" className="btn btn-primary btn-lg mt-2" onClick={() => void backToStaging()}>
              Send more files
            </button>
          </div>
        ) : (
          <>
            <div className="card">
              <QrCard
              qrSvg={state.qrSvg}
              displayCode={state.displayCode}
              url={state.url}
              hint={state.phase === 'ready' ? 'Waiting for your phone — scan to download' : 'Transferring…'}
              pulse={state.phase === 'ready'}
                footer={
                  state.phase === 'transferring' && (
                    <div className="flex items-center gap-3">
                      <div className="progress-track flex-1">
                        <div className="progress-fill" style={{ width: `${pct}%` }} />
                      </div>
                      <span className="flex-none text-[12px]" style={{ color: 'var(--text-secondary)' }}>
                        {formatBytes(state.bytes)} / {formatBytes(state.total)}
                        {state.speed > 0 && ` · ${formatSpeed(state.speed)}`}
                      </span>
                    </div>
                  )
                }
              />
            </div>
            <FileList files={state.files} live={state.phase === 'transferring'} />
            <div className="flex items-center justify-between">
              <button type="button" className="btn btn-ghost" onClick={() => void backToStaging()}>
                <XIcon size={14} />
                Discard session
              </button>
              {state.phase === 'ready' && (
                <button type="button" className="btn btn-secondary" onClick={() => void cancel()}>
                  Cancel transfer
                </button>
              )}
            </div>
          </>
        )}
      </div>
    );
  }

  // ---- Staging ------------------------------------------------------------
  return (
    <div className="view-enter flex flex-col gap-4">
      <div>
        <h1 className="page-title">
          Send files
        </h1>
        <p className="page-sub">
          Send to any phone on the same Wi-Fi — no cables, no cloud, end-to-end encrypted.
        </p>
      </div>

      <Dropzone
        paths={staged}
        onAdd={(paths) => setStaged((prev) => [...prev, ...paths.filter((p) => !prev.includes(p))])}
        onRemove={(path) => setStaged((prev) => prev.filter((p) => p !== path))}
      />

      {state.phase === 'error' && (
        <div
          className="card-muted flex items-center gap-2 p-3 text-[13px]"
          style={{ background: 'var(--red-soft)', borderColor: 'transparent', color: 'var(--red)' }}
        >
          <UploadIcon size={15} />
          {state.error ?? 'Could not start the transfer.'}
        </div>
      )}

      <div className="flex justify-end">
        <button
          type="button"
          className="btn btn-primary btn-lg"
          disabled={staged.filter((p) => !p.startsWith('browser:')).length === 0}
          onClick={begin}
        >
          <UploadIcon size={16} />
          Start transfer
        </button>
      </div>
    </div>
  );
}

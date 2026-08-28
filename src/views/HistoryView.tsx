import { useCallback, useEffect, useState } from 'react';
import { DownloadIcon, FolderIcon, HistoryIcon, UploadIcon } from '../components/Icons';
import { getDownloadsDir, openFolder } from '../lib/ipc';
import { formatBytes, formatRelativeTime } from '../lib/format';
import type { HistoryEntry } from '../types';

interface HistoryViewProps {
  entries: HistoryEntry[];
  onClear: () => void;
}

export function HistoryView({ entries, onClear }: HistoryViewProps) {
  const [downloadsDir, setDownloadsDir] = useState('');

  useEffect(() => {
    getDownloadsDir()
      .then(setDownloadsDir)
      .catch(() => {});
  }, []);

  const openDownloads = useCallback(() => {
    if (downloadsDir) void openFolder(downloadsDir).catch((err) => console.error('Failed to open Downloads folder:', err));
  }, [downloadsDir]);

  return (
    <div className="view-enter flex flex-col gap-4">
      <div className="flex items-end justify-between">
        <div>
          <h1 className="page-title">
            History
          </h1>
          <p className="page-sub">
            Transfers from this device — stored locally, never uploaded anywhere.
          </p>
        </div>
        {entries.length > 0 && (
          <button type="button" className="btn btn-ghost btn-sm" onClick={onClear}>
            Clear history
          </button>
        )}
      </div>

      {entries.length === 0 ? (
        <div className="card flex flex-col items-center gap-3 p-12 text-center">
          <span
            className="flex h-12 w-12 items-center justify-center rounded-full"
            style={{ background: 'var(--surface-2)', color: 'var(--text-muted)' }}
          >
            <HistoryIcon size={22} />
          </span>
          <div>
            <div className="text-[14.5px] font-medium" style={{ color: 'var(--text)' }}>
              No transfers yet
            </div>
            <div className="mt-1 text-[13px]" style={{ color: 'var(--text-secondary)' }}>
              Send or receive something and it will show up here.
            </div>
          </div>
        </div>
      ) : (
        <div className="card flex flex-col overflow-hidden">
          {entries.map((e, i) => {
            const Icon = e.kind === 'send' ? UploadIcon : DownloadIcon;
            const failed = e.status !== 'completed';
            return (
              <div
                key={e.id}
                className="flex items-center gap-3.5 px-4 py-3"
                style={{ borderTop: i > 0 ? '1px solid var(--border)' : 'none' }}
              >
                <span
                  className="flex h-8 w-8 flex-none items-center justify-center rounded-lg"
                  style={{
                    background: e.kind === 'send' ? 'var(--accent-soft)' : 'var(--green-soft)',
                    color: e.kind === 'send' ? 'var(--accent)' : 'var(--green)',
                  }}
                >
                  <Icon size={15} />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[13.5px] font-medium" style={{ color: 'var(--text)' }}>
                    {e.kind === 'send' ? 'Sent' : 'Received'}{' '}
                    {e.fileNames.length > 0
                      ? `${e.fileNames.length} ${e.fileNames.length === 1 ? 'file' : 'files'}`
                      : 'files'}
                    {e.fileNames.length > 0 && (
                      <span style={{ color: 'var(--text-muted)', fontWeight: 400 }}>
                        {' '}
                        — {e.fileNames[0]}
                        {e.fileNames.length > 1 && ` +${e.fileNames.length - 1} more`}
                      </span>
                    )}
                  </span>
                  <span className="block text-[12px]" style={{ color: 'var(--text-muted)' }}>
                    {formatRelativeTime(e.timestamp)} · {formatBytes(e.totalBytes)}
                  </span>
                </span>
                <span
                  className={`chip ${failed ? 'chip-red' : 'chip-green'}`}
                >
                  {e.status === 'completed' ? 'Completed' : e.status === 'cancelled' ? 'Cancelled' : 'Failed'}
                </span>
                {e.kind === 'receive' && e.status === 'completed' && (
                  <button
                    type="button"
                    className="icon-btn"
                    title="Open Downloads folder"
                    aria-label="Open Downloads folder"
                    onClick={() => void openDownloads()}
                  >
                    <FolderIcon size={15} />
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

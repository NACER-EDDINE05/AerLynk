import { CheckIcon, FileIcon } from './Icons';
import { formatBytes } from '../lib/format';
import type { SessionFile } from '../types';

interface FileListProps {
  files: SessionFile[];
  /** True while a transfer is live (shows per-file progress bars). */
  live?: boolean;
}

/** Session file rows with live per-file progress during a transfer. */
export function FileList({ files, live }: FileListProps) {
  if (files.length === 0) return null;

  return (
    <div className="card flex flex-col overflow-hidden">
      {files.map((f, i) => {
        const pct = f.size > 0 ? Math.min(100, Math.round((f.sentBytes / f.size) * 100)) : 0;
        return (
          <div
            key={f.id}
            className="flex items-center gap-3 px-3.5 py-2.5"
            style={{ borderTop: i > 0 ? '1px solid var(--border)' : 'none' }}
          >
            <span
              className="flex h-7 w-7 flex-none items-center justify-center rounded-lg"
              style={{
                background: f.status === 'done' ? 'var(--green-soft)' : 'var(--surface-2)',
                color: f.status === 'done' ? 'var(--green)' : 'var(--text-muted)',
              }}
            >
              {f.status === 'done' ? <CheckIcon size={14} /> : <FileIcon size={14} />}
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[13.5px]" style={{ color: 'var(--text)' }} title={f.originalName}>
                {f.originalName}
              </span>
              {live && f.status !== 'done' && (
                <span className="progress-track mt-1.5 block" aria-label={`${f.originalName} progress`}>
                  <span className="progress-fill block" style={{ width: `${pct}%` }} />
                </span>
              )}
            </span>
            <span className="flex-none text-[12px]" style={{ color: 'var(--text-muted)' }}>
              {f.status === 'failed'
                ? 'Failed'
                : live && f.status === 'active' && f.size > 0
                  ? `${pct}% · ${formatBytes(f.sentBytes)} / ${formatBytes(f.size)}`
                  : f.size > 0
                    ? formatBytes(f.size)
                    : ''}
            </span>
          </div>
        );
      })}
    </div>
  );
}

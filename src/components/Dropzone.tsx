import { useCallback, useMemo, useState } from 'react';
import { FileIcon, PlusIcon, UploadIcon } from './Icons';
import { useDragDrop } from '../hooks/useDragDrop';
import { pickFiles } from '../lib/ipc';
import { basename } from '../lib/format';

interface StagedFile {
  path: string;
  name: string;
}

interface DropzoneProps {
  /** Absolute paths staged for sending (from the OS drag-drop or the picker). */
  paths: string[];
  onAdd: (paths: string[]) => void;
  onRemove: (path: string) => void;
  disabled?: boolean;
}

/**
 * File staging surface: window-wide drag & drop plus click-to-browse.
 * The backend needs real filesystem paths, which Tauri's drag-drop event
 * provides; in browser dev we can only stage names (no session start).
 */
export function Dropzone({ paths, onAdd, onRemove, disabled }: DropzoneProps) {
  const { isDragging } = useDragDrop(
    (dropped) => {
      if (!disabled) onAdd(dropped);
    },
    (files) => {
      // Browser dev fallback: File objects carry no real path, so we stage
      // their names and surface a hint that sending requires the desktop app.
      if (!disabled) onAdd(files.map((f) => `browser:${f.name}`));
    }
  );
  const [pickError, setPickError] = useState('');

  const staged = useMemo<StagedFile[]>(
    () => paths.map((p) => ({ path: p, name: basename(p) })),
    [paths]
  );

  const browse = useCallback(async () => {
    if (disabled) return;
    try {
      const picked = await pickFiles();
      if (picked.length > 0) onAdd(picked);
      setPickError('');
    } catch (error) {
      setPickError(String(error));
    }
  }, [disabled, onAdd]);

  return (
    <div className="flex flex-col gap-3">
      <div
        className={`dropzone flex min-h-[180px] flex-col items-center justify-center gap-2 px-6 py-8 ${isDragging ? 'dragging' : ''}`}
        onClick={browse}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') void browse();
        }}
        role="button"
        tabIndex={disabled ? -1 : 0}
        aria-disabled={disabled}
        aria-label="Add files to send"
      >
        <span
          className="dz-icon flex h-11 w-11 items-center justify-center rounded-full"
          style={{ background: 'var(--accent-soft)', color: 'var(--accent)' }}
        >
          {isDragging ? <UploadIcon size={20} /> : <PlusIcon size={20} />}
        </span>
        <div className="text-[14px] font-medium" style={{ color: 'var(--text)' }}>
          {isDragging ? 'Drop to add files' : 'Drag & drop files here'}
        </div>
        <div className="text-[12.5px]" style={{ color: 'var(--text-muted)' }}>
          or click to browse — files never leave your network
        </div>
      </div>

      {pickError && (
        <div className="text-[12.5px]" style={{ color: 'var(--red)' }}>
          Could not open the file picker: {pickError}
        </div>
      )}

      {staged.length > 0 && (
        <div className="card flex flex-col overflow-hidden">
          {staged.map((f, i) => (
            <div
              key={f.path}
              className="flex items-center gap-3 px-3.5 py-2.5"
              style={{ borderTop: i > 0 ? '1px solid var(--border)' : 'none' }}
            >
              <FileIcon size={16} style={{ color: 'var(--text-muted)', flex: 'none' }} />
              <span className="min-w-0 flex-1 truncate text-[13.5px]" style={{ color: 'var(--text)' }} title={f.path}>
                {f.name}
              </span>
              <button
                type="button"
                className="icon-btn"
                aria-label={`Remove ${f.name}`}
                onClick={() => onRemove(f.path)}
              >
                <PlusIcon size={14} style={{ transform: 'rotate(45deg)' }} />
              </button>
            </div>
          ))}
          <div
            className="px-3.5 py-2 text-[12px]"
            style={{ borderTop: '1px solid var(--border)', color: 'var(--text-muted)' }}
          >
            {staged.length} {staged.length === 1 ? 'file' : 'files'} ready
          </div>
        </div>
      )}
    </div>
  );
}

import { CheckIcon, CopyIcon } from './Icons';
import { useState } from 'react';

interface QrCardProps {
  qrSvg: string;
  displayCode: string;
  url: string;
  /** Short instruction under the code, e.g. "Scan to receive" / "Scan to download". */
  hint: string;
  /** Optional footer line (e.g. progress summary while transferring). */
  footer?: React.ReactNode;
  /** True while waiting for the phone — drives the radar pulse rings. */
  pulse?: boolean;
}

/** The real, scannable QR from the backend plus pairing code and copy-link. */
export function QrCard({ qrSvg, displayCode, url, hint, footer, pulse = false }: QrCardProps) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      /* clipboard unavailable */
    }
  };

  return (
    <div className="flex items-center gap-6 p-6">
      {/* Real QR rendered from the session URL (AES key in fragment), framed
          with scan-corner ticks; radar rings pulse while waiting for the
          phone to connect. */}
      <div className="relative shrink-0" style={{ width: 192, height: 192 }}>
        {pulse && (
          <div className="radar" aria-hidden>
            <span />
            <span />
            <span />
          </div>
        )}
        <div
          className="qr-frame relative h-full w-full rounded-lg p-3.5"
          style={{
            background: '#ffffff',
            border: '1px solid var(--border)',
            boxShadow: pulse ? 'var(--shadow-card), var(--glow)' : 'var(--shadow-card)',
          }}
        >
          <div
            className="qr-box h-full w-full"
            // The SVG is generated locally by our own Rust binary — trusted markup.
            dangerouslySetInnerHTML={{ __html: qrSvg }}
          />
        </div>
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 text-[13px]" style={{ color: 'var(--text-secondary)' }}>
          <span className="pulse-dot" />
          {hint}
        </div>

        <div className="mt-2 flex items-baseline gap-2">
          <span
            className="mono text-[26px] font-semibold tracking-wide"
            style={{ color: 'var(--text)' }}
          >
            {displayCode}
          </span>
          <span className="text-[12px]" style={{ color: 'var(--text-muted)' }}>
            pairing code
          </span>
        </div>

        <div className="mt-4 flex flex-wrap items-center gap-2">
          <button type="button" className="btn btn-secondary btn-sm" onClick={copy} disabled={!url}>
            {copied ? <CheckIcon size={14} /> : <CopyIcon size={14} />}
            {copied ? 'Copied' : 'Copy link'}
          </button>
        </div>

        {url && (
          <div
            className="mono mt-3 truncate text-[11.5px] select-text"
            style={{ color: 'var(--text-muted)' }}
            title={url}
          >
            {url}
          </div>
        )}

        {footer && <div className="mt-4">{footer}</div>}
      </div>
    </div>
  );
}

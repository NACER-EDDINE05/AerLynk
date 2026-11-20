import { CheckIcon, CopyIcon, LinkIcon } from './Icons';
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
  const [manual, setManual] = useState(false);

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
          {url && (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={() => setManual((v) => !v)}
              aria-expanded={manual}
            >
              <LinkIcon size={14} />
              {manual ? 'Hide manual setup' : 'Connect manually'}
            </button>
          )}
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

        {/* Fallback for a phone whose camera cannot focus or whose QR app
            strips the URL fragment (which carries the key). */}
        {manual && url && (
          <div
            className="mt-3 rounded-lg p-3"
            style={{ background: 'var(--surface-2)', border: '1px solid var(--border)' }}
          >
            <div
              className="text-[10.5px] font-medium uppercase"
              style={{ color: 'var(--text-muted)', letterSpacing: '0.1em' }}
            >
              Open this on your phone
            </div>
            <div
              className="mono mt-1.5 text-[11.5px] select-text"
              style={{ color: 'var(--text)', wordBreak: 'break-all' }}
            >
              {url}
            </div>
            <ol
              className="mt-2 flex list-decimal flex-col gap-1 pl-4 text-[12px]"
              style={{ color: 'var(--text-secondary)' }}
            >
              <li>
                Open the whole link, in the same Wi-Fi network. The part after{' '}
                <span className="mono">#</span> is the encryption key.
              </li>
              <li>
                If the page says the key is missing, the link was cut short — use{' '}
                <span style={{ color: 'var(--text)' }}>Copy link</span> and send it across intact.
              </li>
              <li>
                Check that the code on the phone reads{' '}
                <span className="mono" style={{ color: 'var(--text)' }}>
                  {displayCode}
                </span>
                .
              </li>
            </ol>
          </div>
        )}

        {footer && <div className="mt-4">{footer}</div>}
      </div>
    </div>
  );
}

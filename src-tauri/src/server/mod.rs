//! HTTP server for phone sessions (FR-14..17, FR-26..27, SEC-2/8/10/12).
//!
//! Routes (all token-gated; unknown tokens get an indistinguishable 404):
//!   GET  /s/<token>          Send listing page (Phase 4 replaces the shell)
//!   GET  /s/<token>/f/<id>   Encrypted download stream (download.rs)
//!   GET  /r/<token>          Receive page (Phase 4 replaces the shell)
//!   POST /r/<token>          Encrypted multipart upload (upload.rs)
//!   GET  /aesgcm.wasm        The cipher, from OUT_DIR (assets.rs)
//!
//! The phone client must be fully self-contained (SEC-12): the listing page
//! references only inline CSS and the /aesgcm.wasm served from the binary.

pub mod assets;
pub mod download;
pub mod upload;

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use base64::Engine;
use serde::Serialize;
use axum::Router;

use crate::session::{Session, SessionKind, SessionRegistry};

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<SessionRegistry>,
    /// Known-folder Downloads; resolved by the shell in Phase 5 (FR-11).
    pub downloads_dir: PathBuf,
    /// Quarantine for in-flight uploads; partial uploads never appear in
    /// Downloads (SEC-5).
    pub quarantine_dir: PathBuf,
}

pub fn app(state: AppState) -> Router {
    use axum::extract::DefaultBodyLimit;
    Router::new()
        .route("/s/{token}", get(listing))
        .route("/s/{token}/f/{id}", get(download::download_file))
        // The upload route does its own per-session byte accounting (SEC-10);
        // axum's default 2 MiB body limit would otherwise reject large files.
        .route(
            "/r/{token}",
            get(receive_page)
                .post(upload::upload)
                .layer(DefaultBodyLimit::disable()),
        )
        .route("/aesgcm.wasm", get(assets::wasm))
        .with_state(state)
}

/// Resolve a session by token for the given kind, or an indistinguishable 404
/// (FR-16, SEC-2). Touches activity so an in-use session is not reaped.
fn resolve_session(
    state: &AppState,
    kind: SessionKind,
    token: &str,
) -> Result<Arc<Session>, StatusCode> {
    let session = state
        .registry
        .find_by_token(token)
        .ok_or(StatusCode::NOT_FOUND)?;
    if session.kind != kind {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(session)
}

async fn listing(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Response, StatusCode> {
    let session = resolve_session(&state, SessionKind::Send, &token)?;
    let files: Vec<PhoneFile> = session
        .files()
        .into_iter()
        .map(|f| PhoneFile {
            id: f.id,
            name: f.original_name,
            size: f.size,
        })
        .collect();
    let files_b64 = base64::engine::general_purpose::STANDARD_NO_PAD
        .encode(serde_json::to_vec(&files).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?);
    let nonce = nonce_js_array(&session.base_nonce);

    Ok(Html(render_send_page(
        &token,
        &session.display_code,
        &files_b64,
        &nonce,
    ))
    .into_response())
}

async fn receive_page(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Response, StatusCode> {
    let session = resolve_session(&state, SessionKind::Receive, &token)?;
    let nonce = nonce_js_array(&session.base_nonce);
    Ok(Html(render_receive_page(
        &token,
        &session.display_code,
        &nonce,
    ))
    .into_response())
}

#[derive(Serialize)]
struct PhoneFile {
    id: u32,
    name: String,
    size: u64,
}

fn nonce_js_array(base: &[u8; 12]) -> String {
    base.iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn shell_styles() -> &'static str {
    r##"*{box-sizing:border-box}
body{margin:0;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,"Helvetica Neue",Arial,sans-serif;color:#eef2fa;font-size:15px;line-height:1.5;-webkit-font-smoothing:antialiased;-webkit-tap-highlight-color:transparent;background:radial-gradient(1100px 700px at 85% -10%,rgb(77 125 255 / 0.16),transparent 60%),radial-gradient(900px 700px at -10% 105%,rgb(0 190 255 / 0.11),transparent 55%),#06080d}
body::after{content:"";position:fixed;inset:0;pointer-events:none;opacity:0.04;background-image:url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2'/%3E%3C/filter%3E%3Crect width='160' height='160' filter='url(%23n)'/%3E%3C/svg%3E")}
.mono{font-family:ui-monospace,"SF Mono","Cascadia Code",Consolas,monospace;font-variant-numeric:tabular-nums}
header{max-width:560px;margin:0 auto;padding:18px 18px 0;display:flex;align-items:center;justify-content:space-between;gap:12px}
.brand{display:flex;align-items:center;gap:8px;font-weight:650;font-size:14px;letter-spacing:-0.01em}
.brand .mark{width:24px;height:24px;border-radius:8px;background:linear-gradient(135deg,#4d7dff,#1fa7d9);display:inline-flex;align-items:center;justify-content:center;flex:none;box-shadow:inset 0 1px 0 rgb(255 255 255 / 0.25),0 0 16px rgb(77 125 255 / 0.4)}
.brand .mark svg{display:block}
.secure{display:inline-flex;align-items:center;gap:5px;font-size:11px;font-weight:600;letter-spacing:0.02em;color:#4cd98a;background:rgb(76 217 138 / 0.1);border:1px solid rgb(76 217 138 / 0.22);padding:4px 9px;border-radius:999px;flex:none}
main{max-width:560px;margin:0 auto;padding:8px 18px calc(30px + env(safe-area-inset-bottom))}
h1{margin:14px 0 4px;font-size:24px;font-weight:650;letter-spacing:-0.02em;line-height:1.2;background:linear-gradient(120deg,#f4f7ff,#9db6ea);-webkit-background-clip:text;background-clip:text;color:transparent}
p{margin:0 0 12px;color:#a3aec2;font-size:13.5px}
.code-chip{display:inline-block;padding:7px 14px;border:1px solid rgb(91 140 255 / 0.35);border-radius:999px;font-size:12.5px;letter-spacing:0.08em;background:rgb(91 140 255 / 0.09);color:#bcd0ff;margin:0 0 4px}
.card{margin-top:14px;padding:16px;border:1px solid rgb(255 255 255 / 0.08);border-radius:18px;background:rgb(255 255 255 / 0.045);backdrop-filter:blur(16px) saturate(160%);-webkit-backdrop-filter:blur(16px) saturate(160%);box-shadow:inset 0 1px 0 rgb(255 255 255 / 0.06),0 16px 48px rgb(0 0 0 / 0.45)}
.row{display:flex;justify-content:space-between;gap:12px;align-items:center;padding:12px 0;border-bottom:1px solid rgb(255 255 255 / 0.06)}
.row:last-child{border-bottom:none}
.name{min-width:0;flex:1}
.name .t{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-size:14px;font-weight:550}
.name .m{display:block;color:#67728a;font-size:11.5px;margin-top:2px}
.muted{color:#67728a;font-size:12px}
button{border:none;background:linear-gradient(135deg,#4d7dff,#1fa7d9);color:#051028;padding:13px 16px;border-radius:12px;font-weight:650;font-size:14.5px;font-family:inherit;cursor:pointer;width:100%;transition:transform 120ms ease,filter 140ms ease,opacity 120ms ease;-webkit-user-select:none;user-select:none;box-shadow:0 2px 8px rgb(6 12 30 / 0.5),0 10px 30px rgb(47 107 255 / 0.30)}
button:hover:not(:disabled){filter:brightness(1.08)}
button:active:not(:disabled){transform:scale(0.97)}
button:disabled{opacity:0.45;cursor:not-allowed}
.row button{width:auto;padding:9px 14px;font-size:13px;border-radius:10px}
input[type=file]{width:100%;padding:13px;border-radius:12px;border:1.5px dashed rgb(255 255 255 / 0.16);background:rgb(255 255 255 / 0.03);color:#eef2fa;font-size:13px;font-family:inherit}
input[type=file]::file-selector-button{border:none;background:rgb(255 255 255 / 0.08);color:#eef2fa;padding:7px 12px;border-radius:8px;font-family:inherit;font-size:12.5px;font-weight:550;margin-right:10px;cursor:pointer}
progress{width:100%;height:10px;-webkit-appearance:none;appearance:none;border:none;border-radius:5px;background:rgb(255 255 255 / 0.07);overflow:hidden;margin-top:14px}
progress::-webkit-progress-bar{background:rgb(255 255 255 / 0.07);border-radius:5px}
progress::-webkit-progress-value{background:linear-gradient(90deg,#4d7dff,#2fd0ff);border-radius:5px;transition:width 250ms ease;box-shadow:0 0 12px rgb(77 125 255 / 0.6)}
.status{margin-top:12px;font-size:13px;line-height:1.55;white-space:pre-wrap;color:#a3aec2}
.status.muted{color:#67728a}
.status.ok{color:#4cd98a}
.status.error{color:#ff8080}
.trust{display:flex;align-items:center;justify-content:center;gap:6px;margin-top:18px;color:#67728a;font-size:11.5px}"##
}

fn wasm_crypto_runtime() -> &'static str {
    r#"const B64URL = s => s.replace(/-/g,'+').replace(/_/g,'/');
function decodeKeyFromHash(){
  const raw = decodeURIComponent((location.hash||'').replace(/^#/, '').trim());
  if(!raw) throw new Error('Missing key in QR URL fragment.');
  const b64 = B64URL(raw); const pad = '='.repeat((4 - b64.length % 4) % 4);
  const bytes = Uint8Array.from(atob(b64 + pad), c => c.charCodeAt(0));
  if(bytes.length !== 32) throw new Error('Session key must be 32 bytes.');
  return bytes;
}
async function loadCipher(){
  const response = await fetch('/aesgcm.wasm', { cache: 'no-store' });
  if(!response.ok) throw new Error('Unable to load encryption module.');
  const bytes = await response.arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const e = instance.exports;
  if(!e.memory || !e.aerlynk_alloc || !e.aerlynk_free || !e.aerlynk_encrypt || !e.aerlynk_decrypt) throw new Error('Invalid cipher module.');
  // All staging memory comes from the module's OWN allocator (aerlynk_alloc /
  // aerlynk_free). Never write at fixed low addresses: they clobber the
  // module's statics, shadow stack, and allocator metadata, and the module
  // itself allocates while hashing, so its allocator may grow linear memory
  // at any time. Growing a WebAssembly.Memory DETACHES the ArrayBuffer behind
  // any previously created view — so a view is created fresh at every use and
  // never cached across a call.
  const alloc = len => { const p = e.aerlynk_alloc(len); if (!p) throw new Error('Out of cipher memory.'); return p; };
  return {
    encryptChunk(key, nonce, idx, plain){
      const outLen = e.aerlynk_encrypt_len(plain.length);
      const keyPtr = alloc(key.length), noncePtr = alloc(nonce.length);
      const plainPtr = alloc(plain.length), outPtr = alloc(outLen);
      try {
        const m = () => new Uint8Array(e.memory.buffer);
        m().set(key, keyPtr); m().set(nonce, noncePtr); m().set(plain, plainPtr);
        const rc = e.aerlynk_encrypt(keyPtr, key.length, noncePtr, nonce.length, idx >>> 0, Math.floor(idx / 4294967296), plainPtr, plain.length, outPtr);
        if (rc !== 0) throw new Error('Encryption failed.');
        return m().slice(outPtr, outPtr + outLen);
      } finally {
        e.aerlynk_free(keyPtr, key.length); e.aerlynk_free(noncePtr, nonce.length);
        e.aerlynk_free(plainPtr, plain.length); e.aerlynk_free(outPtr, outLen);
      }
    },
    decryptChunk(key, nonce, idx, framed){
      const outLen = e.aerlynk_decrypt_len(framed.length);
      if (!outLen) throw new Error('Malformed encrypted frame.');
      // The 4-byte plaintext-length prefix is framing, not ciphertext. The
      // WASM ABI (see aerlynk_decrypt) expects the bare ciphertext, so skip it.
      const ctLen = framed.length - 4;
      const keyPtr = alloc(key.length), noncePtr = alloc(nonce.length);
      const framePtr = alloc(framed.length), outPtr = alloc(outLen);
      try {
        const m = () => new Uint8Array(e.memory.buffer);
        m().set(key, keyPtr); m().set(nonce, noncePtr); m().set(framed, framePtr);
        const rc = e.aerlynk_decrypt(keyPtr, key.length, noncePtr, nonce.length, idx >>> 0, Math.floor(idx / 4294967296), framePtr + 4, ctLen, outPtr);
        if (rc !== 0) throw new Error('Decryption failed. Check the QR session key.');
        return m().slice(outPtr, outPtr + outLen);
      } finally {
        e.aerlynk_free(keyPtr, key.length); e.aerlynk_free(noncePtr, nonce.length);
        e.aerlynk_free(framePtr, framed.length); e.aerlynk_free(outPtr, outLen);
      }
    }
  };
}"#
}

/// Inline SVG glyphs for the phone pages — no external assets (SEC-12).
const PHONE_MARK_SVG: &str = "<svg width=\"12\" height=\"12\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"#fff\" stroke-width=\"2.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M5 16.5 12 5l7 11.5M7.6 12.4h8.8\"/></svg>";
const PHONE_LOCK_SVG: &str = "<svg width=\"11\" height=\"11\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\"><rect x=\"4\" y=\"11\" width=\"16\" height=\"10\" rx=\"2\"/><path d=\"M8 11V7a4 4 0 0 1 8 0v4\"/></svg>";
const PHONE_SHIELD_SVG: &str = "<svg width=\"12\" height=\"12\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M12 3 5 6v5c0 4.5 3 8.2 7 9.5 4-1.3 7-5 7-9.5V6l-7-3Z\"/></svg>";

fn render_send_page(token: &str, display_code: &str, files_b64: &str, nonce: &str) -> String {
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><meta name="theme-color" content="#06080d"><title>AerLynk</title><style>{styles}</style></head>
<body><header><span class="brand"><span class="mark">{mark}</span>AerLynk</span><span class="secure">{lock} Encrypted</span></header>
<main><h1>Download from AerLynk</h1><p>Session code</p><p class="code-chip mono">{code}</p>
<div class="card"><div id="list"></div><div id="status" class="status muted">Pick a file to download and decrypt on this phone.</div></div>
<div class="trust">{shield} End-to-end encrypted — files never leave your Wi-Fi</div></main>
<script>
const TOKEN = "{token}";
const NONCE = new Uint8Array([{nonce}]);
const FILES = JSON.parse(atob("{files_b64}"));
const fmt = n => n < 1024 ? `${{n}} B` : n < 1024*1024 ? `${{(n/1024).toFixed(1)}} KB` : n < 1024*1024*1024 ? `${{(n/1024/1024).toFixed(1)}} MB` : `${{(n/1024/1024/1024).toFixed(2)}} GB`;
{runtime}
const statusEl = document.getElementById('status');
const listEl = document.getElementById('list');
function setStatus(text, muted=false, tone=''){{ statusEl.textContent=text; statusEl.className='status'+(muted?' muted':'')+(tone?' '+tone:''); }}
for (const f of FILES) {{
  const row = document.createElement('div'); row.className='row';
  const left = document.createElement('div'); left.className='name';
  left.innerHTML = `<span class="t">${{f.name}}</span><span class="m">${{fmt(f.size)}}</span>`;
  const btn = document.createElement('button'); btn.textContent='Download'; btn.onclick=() => downloadFile(f, btn);
  row.append(left, btn); listEl.append(row);
}}
async function downloadFile(file, btn) {{
  btn.disabled = true;
  try {{
    setStatus(`Downloading ${{file.name}}...`, true);
    const key = decodeKeyFromHash();
    const cipher = await loadCipher();
    const res = await fetch(`/s/${{TOKEN}}/f/${{file.id}}`);
    if (!res.ok) throw new Error('Download failed.');
    const total = Number(res.headers.get('content-length') || 0);
    const reader = res.body.getReader();
    const chunks = []; let received = 0;
    while (true) {{
      const r = await reader.read(); if (r.done) break;
      chunks.push(r.value); received += r.value.length;
      if (total > 0) setStatus(`Downloading ${{file.name}}... ${{Math.round(received*100/total)}}%`, true);
    }}
    const framed = new Uint8Array(received); let off = 0;
    for (const c of chunks) {{ framed.set(c, off); off += c.length; }}
    const plainParts = []; let pos = 0; let idx = 0;
    while (pos < framed.length) {{
      if (pos + 4 > framed.length) throw new Error('Malformed encrypted stream.');
      const ptLen = (framed[pos]<<24) | (framed[pos+1]<<16) | (framed[pos+2]<<8) | framed[pos+3];
      const frameLen = 4 + ptLen + 16;
      if (pos + frameLen > framed.length) throw new Error('Malformed encrypted stream.');
      const part = cipher.decryptChunk(key, NONCE, idx++, framed.slice(pos, pos + frameLen));
      plainParts.push(part); pos += frameLen;
    }}
    const blob = new Blob(plainParts, {{ type:'application/octet-stream' }});
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a'); a.href = url; a.download = file.name; a.click();
    URL.revokeObjectURL(url);
    setStatus(`Downloaded ${{file.name}} successfully.`, false, 'ok');
  }} catch (e) {{
    setStatus(e?.message || 'Download failed.', false, 'error');
  }} finally {{ btn.disabled = false; }}
}}
</script></body></html>"##,
        styles = shell_styles(),
        code = display_code,
        token = token,
        nonce = nonce,
        files_b64 = files_b64,
        runtime = wasm_crypto_runtime(),
        mark = PHONE_MARK_SVG,
        lock = PHONE_LOCK_SVG,
        shield = PHONE_SHIELD_SVG
    )
}

fn render_receive_page(token: &str, display_code: &str, nonce: &str) -> String {
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><meta name="theme-color" content="#06080d"><title>AerLynk</title><style>{styles}</style></head>
<body><header><span class="brand"><span class="mark">{mark}</span>AerLynk</span><span class="secure">{lock} Encrypted</span></header>
<main><h1>Upload to AerLynk</h1><p>Session code</p><p class="code-chip mono">{code}</p>
<div class="card"><input id="picker" type="file" multiple><div style="height:10px"></div><button id="send">Upload selected files</button>
<div style="height:10px"></div><progress id="progress" max="100" value="0"></progress><div id="status" class="status muted">Select files, then upload.</div></div>
<div class="trust">{shield} End-to-end encrypted — files never leave your Wi-Fi</div></main>
<script>
const TOKEN = "{token}";
const NONCE = new Uint8Array([{nonce}]);
const CHUNK = 1024 * 1024;
{runtime}
const picker = document.getElementById('picker');
const send = document.getElementById('send');
const progress = document.getElementById('progress');
const statusEl = document.getElementById('status');
function setStatus(text, muted=false, tone=''){{ statusEl.textContent=text; statusEl.className='status'+(muted?' muted':'')+(tone?' '+tone:''); }}
async function encryptFile(file, key, cipher) {{
  const raw = new Uint8Array(await file.arrayBuffer());
  const parts = []; let idx = 0;
  for (let i = 0; i < raw.length; i += CHUNK) {{
    const plain = raw.slice(i, Math.min(i + CHUNK, raw.length));
    parts.push(cipher.encryptChunk(key, NONCE, idx++, plain));
  }}
  return new Blob(parts, {{ type: 'application/octet-stream' }});
}}
send.onclick = async () => {{
  if (!picker.files || picker.files.length === 0) {{ setStatus('Pick at least one file first.'); return; }}
  send.disabled = true;
  try {{
    progress.value = 0;
    const key = decodeKeyFromHash();
    const cipher = await loadCipher();
    const form = new FormData();
    // Names + plaintext sizes let the PC verify every upload is complete
    // before releasing it from quarantine (SEC-5, FR-12).
    form.append('manifest', JSON.stringify(Array.from(picker.files).map(f => ({{ name: f.name, size: f.size }}))));
    for (const file of picker.files) {{
      setStatus(`Encrypting ${{file.name}}...`, true);
      const encrypted = await encryptFile(file, key, cipher);
      form.append('files', encrypted, file.name);
    }}
    setStatus('Uploading...', true);
    await new Promise((resolve, reject) => {{
      const xhr = new XMLHttpRequest();
      xhr.open('POST', `/r/${{TOKEN}}`);
      xhr.upload.onprogress = (e) => {{ if (e.lengthComputable) progress.value = Math.round((e.loaded / e.total) * 100); }};
      xhr.onload = () => xhr.status >= 200 && xhr.status < 300 ? resolve(xhr.responseText) : reject(new Error('Upload failed.'));
      xhr.onerror = () => reject(new Error('Upload failed.'));
      xhr.send(form);
    }});
    setStatus('Upload finished. You can close this page now.', false, 'ok');
  }} catch (e) {{
    setStatus(e?.message || 'Upload failed.', false, 'error');
  }} finally {{
    send.disabled = false;
  }}
}};
</script></body></html>"##,
        styles = shell_styles(),
        code = display_code,
        token = token,
        nonce = nonce,
        runtime = wasm_crypto_runtime(),
        mark = PHONE_MARK_SVG,
        lock = PHONE_LOCK_SVG,
        shield = PHONE_SHIELD_SVG
    )
}

/// Constant-time 404 body so unknown and expired tokens are indistinguishable
/// even in the response body (FR-16): routes use `StatusCode::NOT_FOUND` with
/// a plain "Not found" body via `resolve_session`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{FileStatus, SessionKind, DEFAULT_INACTIVITY_TIMEOUT};
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::net::TcpListener;

    static STATE_SEQ: AtomicU64 = AtomicU64::new(0);

    fn state() -> (AppState, Arc<SessionRegistry>) {
        let seq = STATE_SEQ.fetch_add(1, Ordering::Relaxed);
        let registry = Arc::new(SessionRegistry::new(DEFAULT_INACTIVITY_TIMEOUT));
        let downloads = std::env::temp_dir().join(format!("aerlynk-dl-{}-{seq}", std::process::id()));
        let quarantine = std::env::temp_dir().join(format!("aerlynk-q-{}-{seq}", std::process::id()));
        std::fs::create_dir_all(&downloads).unwrap();
        std::fs::create_dir_all(&quarantine).unwrap();
        (
            AppState {
                registry: registry.clone(),
                downloads_dir: downloads,
                quarantine_dir: quarantine,
            },
            registry,
        )
    }

    #[tokio::test]
    async fn unknown_token_gets_plain_404_everywhere() {
        let (st, _) = state();
        let app = app(st);
        use tower::ServiceExt;
        for path in [
            "/s/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "/s/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA/f/0",
            "/r/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ] {
            let resp = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .uri(path)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }

    #[tokio::test]
    async fn wrong_kind_token_is_indistinguishable_404() {
        let (st, reg) = state();
        let send = reg.create_send_session(vec![]);
        let recv = reg.create_receive_session();
        let app = app(st);
        use tower::ServiceExt;
        // A Send token must not open the receive page.
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/r/{}", send.token))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        // A Receive token must not open the send listing.
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/s/{}", recv.token))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn send_listing_serves_for_valid_token() {
        let (st, reg) = state();
        let s = reg.create_send_session(vec![]);
        let app = app(st);
        use tower::ServiceExt;
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/s/{}", s.token))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// Deterministic pseudo-random bytes for test payloads.
    fn payload(n: usize, seed: u8) -> Vec<u8> {
        let mut v = Vec::with_capacity(n);
        let mut x = seed as u64;
        for _ in 0..n {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            v.push((x >> 16) as u8);
        }
        v
    }

    async fn collect(resp: Response) -> Vec<u8> {
        use http_body_util::BodyExt;
        let body = resp.into_body();
        let collected = body.collect().await.unwrap();
        collected.to_bytes().to_vec()
    }

    #[tokio::test]
    async fn download_round_trips_encrypted_and_decrypts() {
        let (st, reg) = state();
        let dir = std::env::temp_dir().join(format!("aerlynk-src-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("holiday-photo.jpg");
        let pt = payload(3 * 1024 * 1024 + 777, 42);
        std::fs::write(&src, &pt).unwrap();
        let size = pt.len() as u64;

        let s = reg.create_send_session(vec![("holiday-photo.jpg".into(), size, src)]);
        let app = app(st);
        use tower::ServiceExt;
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/s/{}/f/0", s.token))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Content-Length must equal the deterministic ciphertext size.
        let cl = resp
            .headers()
            .get(axum::http::header::CONTENT_LENGTH)
            .unwrap()
            .to_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let full = size / aerlynk_crypto::MAX_CHUNK_PLAINTEXT as u64;
        let rem = size % aerlynk_crypto::MAX_CHUNK_PLAINTEXT as u64;
        let expected = full * (4 + aerlynk_crypto::MAX_CHUNK_PLAINTEXT as u64 + aerlynk_crypto::TAG_LEN as u64)
            + if rem > 0 { 4 + rem + aerlynk_crypto::TAG_LEN as u64 } else { 0 };
        assert_eq!(cl, expected);

        let body = collect(resp).await;
        let decrypted = aerlynk_crypto::decrypt_all(&s.key, &s.base_nonce, &body).unwrap();
        assert_eq!(decrypted, pt);
        // Wire bytes must NOT contain plaintext (SEC-13).
        assert!(!body.windows(pt.len().min(64)).any(|w| w == &pt[..pt.len().min(64)]));
    }

    #[tokio::test]
    async fn download_unknown_file_id_is_404() {
        let (st, reg) = state();
        let dir = std::env::temp_dir().join(format!("aerlynk-src-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("a.bin");
        std::fs::write(&src, b"data").unwrap();
        let s = reg.create_send_session(vec![("a.bin".into(), 4, src)]);
        let app = app(st);
        use tower::ServiceExt;
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/s/{}/f/999", s.token))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Build a multipart/form-data body with one or more file parts.
    fn multipart_body(boundary: &str, parts: &[(&str, &str, &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        for (name, filename, data) in parts {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n")
                    .as_bytes(),
            );
            body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
            body.extend_from_slice(data);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        body
    }

    fn upload_request(token: &str, body: Vec<u8>, boundary: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::builder()
            .method("POST")
            .uri(format!("/r/{token}"))
            .header(
                axum::http::header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(axum::body::Body::from(body))
            .unwrap()
    }

    #[tokio::test]
    async fn upload_round_trips_to_downloads_byte_for_byte() {
        let (st, reg) = state();
        let s = reg.create_receive_session();
        let pt = payload(2 * 1024 * 1024 + 999, 7);
        let framed = aerlynk_crypto::encrypt_all(&s.key, &s.base_nonce, &pt);
        let body = multipart_body("xyz", &[("file", "holiday.jpg", &framed)]);
        let app = app(st.clone());
        use tower::ServiceExt;
        let resp = app.oneshot(upload_request(&s.token, body, "xyz")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = collect(resp).await;
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json[0]["status"], "done");

        let dest = st.downloads_dir.join("holiday.jpg");
        assert_eq!(std::fs::read(&dest).unwrap(), pt);
        // Manifest registered with the final path.
        let file = s.file_by_id(json[0]["id"].as_u64().unwrap() as u32).unwrap();
        assert_eq!(file.path.as_deref(), Some(dest.as_path()));
        assert_eq!(*file.status.lock().unwrap(), FileStatus::Done);
    }

    #[tokio::test]
    async fn upload_collision_suffixes_instead_of_overwriting() {
        let (st, reg) = state();
        let s = reg.create_receive_session();
        let pt = payload(1000, 1);
        let framed = aerlynk_crypto::encrypt_all(&s.key, &s.base_nonce, &pt);
        std::fs::write(st.downloads_dir.join("clip.mp4"), b"first").unwrap();
        let body = multipart_body("b1", &[("file", "clip.mp4", &framed)]);
        let app = app(st.clone());
        use tower::ServiceExt;
        let resp = app.oneshot(upload_request(&s.token, body, "b1")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(std::fs::read(st.downloads_dir.join("clip.mp4")).unwrap(), b"first");
        assert_eq!(std::fs::read(st.downloads_dir.join("clip (1).mp4")).unwrap(), pt);
    }

    #[tokio::test]
    async fn upload_sanitizes_hostile_filenames() {
        let (st, reg) = state();
        let s = reg.create_receive_session();
        let pt = payload(500, 9);
        let framed = aerlynk_crypto::encrypt_all(&s.key, &s.base_nonce, &pt);
        let body = multipart_body("b2", &[("file", "../../evil.txt", &framed)]);
        let app = app(st.clone());
        use tower::ServiceExt;
        let resp = app.oneshot(upload_request(&s.token, body, "b2")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // SEC-4/6: the file lands inside Downloads under a sanitized name.
        assert_eq!(std::fs::read(st.downloads_dir.join("evil.txt")).unwrap(), pt);
        assert!(!st.downloads_dir.join("..").join("evil.txt").exists());
    }

    #[tokio::test]
    async fn upload_tampered_ciphertext_is_rejected_and_leaves_no_file() {
        let (st, reg) = state();
        let s = reg.create_receive_session();
        let pt = payload(2000, 3);
        let mut framed = aerlynk_crypto::encrypt_all(&s.key, &s.base_nonce, &pt);
        let mid = framed.len() / 2;
        framed[mid] ^= 0x80;
        let body = multipart_body("b3", &[("file", "evil.bin", &framed)]);
        let app = app(st.clone());
        use tower::ServiceExt;
        let resp = app.oneshot(upload_request(&s.token, body, "b3")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json: serde_json::Value = serde_json::from_slice(&collect(resp).await).unwrap();
        assert_eq!(json[0]["status"], "failed");
        assert!(!st.downloads_dir.join("evil.bin").exists());
        // Quarantine must be empty of the failed file.
        let leftovers: Vec<_> = std::fs::read_dir(&st.quarantine_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(leftovers.is_empty(), "quarantine must be cleaned on failure");
    }

    #[tokio::test]
    async fn upload_unknown_token_is_404() {
        let (st, _) = state();
        let body = multipart_body("b4", &[("file", "x.bin", b"junk")]);
        let app = app(st);
        use tower::ServiceExt;
        let resp = app
            .oneshot(upload_request("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", body, "b4"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn wasm_asset_is_served_and_valid() {
        let (st, _) = state();
        let app = app(st);
        use tower::ServiceExt;
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/aesgcm.wasm")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = collect(resp).await;
        assert_eq!(&bytes[..4], b"\0asm");
    }

    /// Current process working set (Windows). Used to prove flat memory: the
    /// RSS delta during a multi-GB transfer must stay far below the file size
    /// (NFR-1/2 exit criterion).
    fn working_set_bytes() -> u64 {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        unsafe {
            let h = GetCurrentProcess();
            let mut pmc: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            GetProcessMemoryInfo(h, &mut pmc, pmc.cb);
            pmc.WorkingSetSize as u64
        }
    }

    /// Exit criterion: a multi-GB file moves end-to-end over a real socket
    /// with flat memory. Slower in debug; run explicitly with
    /// `cargo test -p aerlynk -- --ignored`.
    #[tokio::test]
    #[ignore = "multi-GB transfer; run explicitly as an exit-criteria check"]
    async fn multi_gb_download_streams_with_flat_memory() {
        let (st, reg) = state();
        let size = 2u64 * 1024 * 1024 * 1024; // 2 GiB
        let dir = std::env::temp_dir().join(format!("aerlynk-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("big.bin");
        let f = tokio::fs::File::create(&src).await.unwrap();
        f.set_len(size).await.unwrap(); // all zeros; sparse-ish, cheap
        drop(f);

        let s = reg.create_send_session(vec![("big.bin".into(), size, src)]);

        // Real socket server on an ephemeral port.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server_app = app(st.clone());
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, server_app).await;
        });

        let before = working_set_bytes();
        let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .unwrap();
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        stream
            .write_all(
                format!("GET /s/{}/f/0 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n", s.token)
                    .as_bytes(),
            )
            .await
            .unwrap();

        // Read headers.
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).await.unwrap();
            head.push(byte[0]);
        }
        let head_str = String::from_utf8_lossy(&head);
        assert!(
            head_str.starts_with("HTTP/1.1 200"),
            "unexpected head: {head_str}"
        );
        let cl = head_str
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse::<u64>().unwrap())
            })
            .expect("content-length present");

        // Stream the body, frame-by-frame, decrypting and discarding.
        // Memory must stay flat: no buffer scales with the file size.
        let mut pending = Vec::with_capacity(4 + aerlynk_crypto::MAX_CHUNK_PLAINTEXT + 16);
        let mut total = 0u64;
        let mut decrypted = 0u64;
        let mut idx = 0u64;
        let mut buf = [0u8; 1 << 16];
        loop {
            let n = stream.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            total += n as u64;
            pending.extend_from_slice(&buf[..n]);
            loop {
                if pending.len() < 4 {
                    break;
                }
                let pt_len =
                    u32::from_be_bytes(pending[..4].try_into().unwrap()) as usize;
                if pt_len > aerlynk_crypto::MAX_CHUNK_PLAINTEXT {
                    panic!("oversized frame in stream");
                }
                if pending.len() < 4 + pt_len + aerlynk_crypto::TAG_LEN {
                    break;
                }
                let ct = pending[4..4 + pt_len + aerlynk_crypto::TAG_LEN].to_vec();
                pending.drain(..4 + pt_len + aerlynk_crypto::TAG_LEN);
                let pt = aerlynk_crypto::decrypt_chunk(&s.key, &s.base_nonce, idx, &ct)
                    .unwrap_or_else(|e| panic!("decrypt chunk {idx}: {e}"));
                assert_eq!(pt.len(), pt_len);
                assert!(pt.iter().all(|b| *b == 0), "chunk {idx} not zeros");
                decrypted += pt_len as u64;
                idx += 1;
            }
        }
        let after = working_set_bytes();

        assert_eq!(total, cl, "body length must match Content-Length");
        assert_eq!(decrypted, size, "all plaintext must round-trip");
        let delta = after.saturating_sub(before);
        assert!(
            delta < 300 * 1024 * 1024,
            "memory grew by {delta} bytes for a {size}-byte file — not flat"
        );
        eprintln!("OK: {size} bytes streamed, RSS delta {delta} bytes");

        server_task.abort();
        let _ = server_task.await;
    }
}

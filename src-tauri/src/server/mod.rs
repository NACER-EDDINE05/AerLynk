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
use axum::routing::{get, post};
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
        // End-of-batch marker: a receive session cannot infer completion from
        // its manifest (files are appended as they arrive), so the phone says
        // when it is done uploading.
        .route("/r/{token}/complete", post(upload::complete))
        // The logotype for the phone pages, served from the binary so the page
        // never reaches the internet for an asset (SEC-12).
        .route("/brand/mark.png", get(brand_mark))
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

/// The AerLynk emblem shown in the phone header, embedded at compile time so
/// the page stays fully self-contained (SEC-12).
static BRAND_MARK_PNG: &[u8] = include_bytes!("../../../public/brand/mark-192.png");

async fn brand_mark() -> Response {
    (
        [
            (axum::http::header::CONTENT_TYPE, "image/png"),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=86400",
            ),
        ],
        BRAND_MARK_PNG,
    )
        .into_response()
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

/// The halftone skin layered over the shell styles: warm paper, ink rules,
/// hard offset shadows and dot screens. It only sets colour, borders and
/// shadows, so the layout rules above stay authoritative.
fn halftone_skin() -> &'static str {
    r##"body{background-color:#f3efe4;background-image:radial-gradient(rgb(23 22 15 / 0.08) 1px,transparent 1.15px),radial-gradient(rgb(23 22 15 / 0.055) 1px,transparent 1.15px);background-size:7px 7px,13px 13px;background-position:0 0,3px 3px;color:#17160f}
.brand{color:#17160f;font-weight:700}
.brand .mark{width:26px;height:26px;border-radius:7px;border:1.5px solid rgb(23 22 15 / 0.45);background:#fff;box-shadow:2px 2px 0 rgb(23 22 15 / 0.16);object-fit:contain}
.secure{color:#10693c;background:rgb(16 105 60 / 0.10);border:1.5px solid #10693c;border-radius:4px;letter-spacing:0.09em;text-transform:uppercase;font-size:10px}
h1{color:#17160f;background:none;-webkit-text-fill-color:currentColor;font-weight:700}
p{color:#4b4738}
.code-chip{border:1.5px solid #17160f;border-radius:4px;background:#fbf8f0;color:#17160f;letter-spacing:0.14em;font-weight:600}
.card{background-color:#fbf8f0;background-image:radial-gradient(rgb(23 22 15 / 0.05) 1px,transparent 1.15px);background-size:6px 6px;border:1.5px solid rgb(23 22 15 / 0.5);border-radius:10px;box-shadow:4px 4px 0 rgb(23 22 15 / 0.14);backdrop-filter:none;-webkit-backdrop-filter:none}
.row{border-bottom:1px solid rgb(23 22 15 / 0.14)}
.name .t{color:#17160f;font-weight:600}
.name .m{color:#7c7660}
.muted{color:#7c7660}
button{background-color:#1450c8;background-image:radial-gradient(rgb(255 255 255 / 0.22) 1px,transparent 1.1px);background-size:3px 3px;color:#fdfbf5;border:1.5px solid #0b2a6b;border-radius:6px;box-shadow:3px 3px 0 rgb(20 80 200 / 0.3);font-weight:650}
button:hover:not(:disabled){filter:none;background-color:#103fa0}
button:active:not(:disabled){transform:none;box-shadow:none}
input[type=file]{border:2px dashed rgb(23 22 15 / 0.4);border-radius:8px;background-color:#fbf8f0;background-image:radial-gradient(rgb(23 22 15 / 0.05) 1px,transparent 1.15px);background-size:8px 8px;color:#17160f}
input[type=file]::file-selector-button{background:#f6f2e7;border:1.5px solid rgb(23 22 15 / 0.45);color:#17160f;border-radius:4px}
progress{height:12px;border-radius:0;border:1.5px solid rgb(23 22 15 / 0.5);background:#f6f2e7}
progress::-webkit-progress-bar{background:#f6f2e7;border-radius:0}
progress::-webkit-progress-value{background-color:#1450c8;background-image:radial-gradient(rgb(255 255 255 / 0.45) 1px,transparent 1.15px);background-size:4px 4px;border-radius:0;box-shadow:none}
.status{color:#4b4738}
.status.muted{color:#7c7660}
.status.ok{color:#10693c}
.status.error{color:#b02a2a}
.trust{color:#7c7660}"##
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
  // ONE reusable scratch region per role, allocated on first use and grown only
  // when a larger chunk arrives. A fresh alloc/free pair for every 1 MiB chunk
  // means thousands of allocator round-trips per gigabyte on top of the AES
  // work; reusing the region keeps phone memory flat and hands the CPU budget
  // to the cipher. These blocks are never freed: the page holds them for its
  // lifetime, which is exactly one session.
  const keyPtr = alloc(64), noncePtr = alloc(32);
  let inPtr = 0, inCap = 0, outPtr = 0, outCap = 0;
  const grow = (ptr, cap, needed) => {
    if (needed <= cap) return [ptr, cap];
    const next = Math.max(needed, 256 * 1024);
    if (ptr) e.aerlynk_free(ptr, cap);
    return [alloc(next), next];
  };
  const view = () => new Uint8Array(e.memory.buffer);
  return {
    encryptChunk(key, nonce, idx, plain){
      const outLen = e.aerlynk_encrypt_len(plain.length);
      let s = grow(inPtr, inCap, plain.length); inPtr = s[0]; inCap = s[1];
      s = grow(outPtr, outCap, outLen); outPtr = s[0]; outCap = s[1];
      const stage = view();
      stage.set(key, keyPtr); stage.set(nonce, noncePtr); stage.set(plain, inPtr);
      const rc = e.aerlynk_encrypt(keyPtr, key.length, noncePtr, nonce.length, idx >>> 0, Math.floor(idx / 4294967296), inPtr, plain.length, outPtr);
      if (rc !== 0) throw new Error('Encryption failed.');
      // The cipher may grow linear memory, which DETACHES any existing view:
      // always copy the result out through a freshly created view.
      return view().slice(outPtr, outPtr + outLen);
    },
    decryptChunk(key, nonce, idx, framed){
      const outLen = e.aerlynk_decrypt_len(framed.length);
      if (!outLen) throw new Error('Malformed encrypted frame.');
      // The 4-byte plaintext-length prefix is framing, not ciphertext. The
      // WASM ABI (see aerlynk_decrypt) expects the bare ciphertext, so skip it.
      const ctLen = framed.length - 4;
      let s = grow(inPtr, inCap, framed.length); inPtr = s[0]; inCap = s[1];
      s = grow(outPtr, outCap, outLen); outPtr = s[0]; outCap = s[1];
      const stage = view();
      stage.set(key, keyPtr); stage.set(nonce, noncePtr); stage.set(framed, inPtr);
      const rc = e.aerlynk_decrypt(keyPtr, key.length, noncePtr, nonce.length, idx >>> 0, Math.floor(idx / 4294967296), inPtr + 4, ctLen, outPtr);
      if (rc !== 0) throw new Error('Decryption failed. Check the QR session key.');
      return view().slice(outPtr, outPtr + outLen);
    }
  };
}"#
}

/// Inline SVG glyphs for the phone pages — no external assets (SEC-12).
const PHONE_LOCK_SVG: &str = "<svg width=\"11\" height=\"11\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\"><rect x=\"4\" y=\"11\" width=\"16\" height=\"10\" rx=\"2\"/><path d=\"M8 11V7a4 4 0 0 1 8 0v4\"/></svg>";
const PHONE_SHIELD_SVG: &str = "<svg width=\"12\" height=\"12\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M12 3 5 6v5c0 4.5 3 8.2 7 9.5 4-1.3 7-5 7-9.5V6l-7-3Z\"/></svg>";

fn render_send_page(token: &str, display_code: &str, files_b64: &str, nonce: &str) -> String {
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><meta name="theme-color" content="#06080d"><title>AerLynk</title><style>{styles}{skin}</style></head>
<body><header><span class="brand"><img class="mark" src="/brand/mark.png" alt="" width="26" height="26">AerLynk</span><span class="secure">{lock} Encrypted</span></header>
<main><h1>Download from AerLynk</h1><p>Session code</p><p class="code-chip mono">{code}</p>
<div class="card"><div id="list"></div><div id="status" class="status muted">Pick a file to download and decrypt on this phone.</div></div>
<div class="trust">{shield} End-to-end encrypted — files never leave your Wi-Fi</div></main>
<script>
const TOKEN = "{token}";
const NONCE = new Uint8Array([{nonce}]);
const FILES = JSON.parse(atob("{files_b64}"));
{runtime}
{script}
</script></body></html>"##,
        styles = shell_styles(),
        skin = halftone_skin(),
        code = display_code,
        token = token,
        nonce = nonce,
        files_b64 = files_b64,
        runtime = wasm_crypto_runtime(),
        script = SEND_PAGE_SCRIPT,
        lock = PHONE_LOCK_SVG,
        shield = PHONE_SHIELD_SVG
    )
}

/// Client script for the send page. Frames are decrypted as they arrive, so a
/// multi-GB download holds the plaintext once instead of buffering the whole
/// ciphertext stream and then a second full copy of the file (the old path
/// needed roughly three times the file size and killed phone tabs).
const SEND_PAGE_SCRIPT: &str = r##"const CHUNK_LIMIT = 8 * 1024 * 1024;
const fmt = n => n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KB` : n < 1073741824 ? `${(n / 1048576).toFixed(1)} MB` : `${(n / 1073741824).toFixed(2)} GB`;
const statusEl = document.getElementById('status');
const listEl = document.getElementById('list');
function setStatus(text, muted = false, tone = '') { statusEl.textContent = text; statusEl.className = 'status' + (muted ? ' muted' : '') + (tone ? ' ' + tone : ''); }
for (const f of FILES) {
  const row = document.createElement('div'); row.className = 'row';
  const left = document.createElement('div'); left.className = 'name';
  const title = document.createElement('span'); title.className = 't'; title.textContent = f.name;
  const meta = document.createElement('span'); meta.className = 'm'; meta.textContent = fmt(f.size);
  left.append(title, meta);
  const btn = document.createElement('button'); btn.type = 'button'; btn.textContent = 'Download';
  btn.onclick = () => downloadFile(f, btn);
  row.append(left, btn);
  listEl.append(row);
}
async function downloadFile(file, btn) {
  btn.disabled = true;
  try {
    setStatus(`Downloading ${file.name}...`, true);
    const key = decodeKeyFromHash();
    const cipher = await loadCipher();
    const res = await fetch(`/s/${TOKEN}/f/${file.id}`);
    if (!res.ok) throw new Error('Download failed.');
    const total = Number(res.headers.get('content-length') || 0);
    const reader = res.body.getReader();
    const parts = [];
    let pending = new Uint8Array(0), off = 0, idx = 0, received = 0;
    for (;;) {
      const read = await reader.read();
      if (read.done) break;
      received += read.value.length;
      if (pending.length === off) {
        pending = read.value; off = 0;
      } else {
        const keep = pending.length - off;
        const merged = new Uint8Array(keep + read.value.length);
        merged.set(pending.subarray(off), 0);
        merged.set(read.value, keep);
        pending = merged; off = 0;
      }
      while (pending.length - off >= 4) {
        const ptLen = (pending[off] << 24) | (pending[off + 1] << 16) | (pending[off + 2] << 8) | pending[off + 3];
        if (ptLen < 0 || ptLen > CHUNK_LIMIT) throw new Error('Malformed encrypted stream.');
        const frameLen = 4 + ptLen + 16;
        if (pending.length - off < frameLen) break;
        parts.push(cipher.decryptChunk(key, NONCE, idx++, pending.subarray(off, off + frameLen)));
        off += frameLen;
      }
      if (total > 0) setStatus(`Downloading ${file.name}... ${Math.min(99, Math.round(received * 100 / total))}%`, true);
    }
    if (pending.length - off > 0) throw new Error('Malformed encrypted stream.');
    if (parts.length === 0 && received > 0) throw new Error('No file data received.');
    const blob = new Blob(parts, { type: 'application/octet-stream' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url; a.download = file.name; a.rel = 'noopener';
    document.body.appendChild(a); a.click(); a.remove();
    // Revoke later: Safari cancels a download whose blob URL is freed at once.
    setTimeout(() => URL.revokeObjectURL(url), 60000);
    setStatus(`Downloaded ${file.name} successfully.`, false, 'ok');
  } catch (e) {
    setStatus(e && e.message ? e.message : 'Download failed.', false, 'error');
  } finally {
    btn.disabled = false;
  }
}"##;

fn render_receive_page(token: &str, display_code: &str, nonce: &str) -> String {
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><meta name="theme-color" content="#06080d"><title>AerLynk</title><style>{styles}{skin}</style></head>
<body><header><span class="brand"><img class="mark" src="/brand/mark.png" alt="" width="26" height="26">AerLynk</span><span class="secure">{lock} Encrypted</span></header>
<main><h1>Upload to AerLynk</h1><p>Session code</p><p class="code-chip mono">{code}</p>
<div class="card"><input id="picker" type="file" multiple><div style="height:10px"></div><button id="send">Upload selected files</button>
<div style="height:10px"></div><progress id="progress" max="100" value="0"></progress><div id="status" class="status muted">Select one or more files, then upload.</div></div>
<div class="trust">{shield} End-to-end encrypted — files never leave your Wi-Fi</div></main>
<script>
const TOKEN = "{token}";
const NONCE = new Uint8Array([{nonce}]);
{runtime}
{script}
</script></body></html>"##,
        styles = shell_styles(),
        skin = halftone_skin(),
        code = display_code,
        token = token,
        nonce = nonce,
        runtime = wasm_crypto_runtime(),
        script = RECEIVE_PAGE_SCRIPT,
        lock = PHONE_LOCK_SVG,
        shield = PHONE_SHIELD_SVG
    )
}

/// Client script for the receive page. Each file is read back in 1 MiB slices
/// (never `file.arrayBuffer()`, which materialises a multi-GB file in a phone
/// tab), encrypted frame by frame, and sent as its own request so one dropped
/// connection or rejected file cannot discard the rest of the batch.
const RECEIVE_PAGE_SCRIPT: &str = r##"const CHUNK = 1024 * 1024;
const picker = document.getElementById('picker');
const send = document.getElementById('send');
const progress = document.getElementById('progress');
const statusEl = document.getElementById('status');
function setStatus(text, muted = false, tone = '') { statusEl.textContent = text; statusEl.className = 'status' + (muted ? ' muted' : '') + (tone ? ' ' + tone : ''); }
function pct(done, total) { return total > 0 ? Math.max(0, Math.min(100, Math.round(done * 100 / total))) : 0; }
async function encryptFile(file, key, cipher, onProgress) {
  const parts = [];
  let idx = 0;
  for (let at = 0; at < file.size; at += CHUNK) {
    const end = Math.min(at + CHUNK, file.size);
    const plain = new Uint8Array(await file.slice(at, end).arrayBuffer());
    parts.push(cipher.encryptChunk(key, NONCE, idx++, plain));
    onProgress(end);
  }
  return new Blob(parts, { type: 'application/octet-stream' });
}
function postFile(blob, file, onProgress) {
  return new Promise((resolve, reject) => {
    const form = new FormData();
    // Name + plaintext size let the PC verify the upload is complete before
    // releasing it from quarantine (SEC-5, FR-12).
    form.append('manifest', JSON.stringify([{ name: file.name, size: file.size }]));
    form.append('files', blob, file.name);
    const xhr = new XMLHttpRequest();
    xhr.open('POST', `/r/${TOKEN}`);
    xhr.upload.onprogress = (e) => { if (e.lengthComputable) onProgress(e.loaded, e.total); };
    xhr.onload = () => {
      if (xhr.status < 200 || xhr.status >= 300) { reject(new Error('Upload failed (' + xhr.status + ').')); return; }
      try { resolve(JSON.parse(xhr.responseText || '[]')); } catch (e) { resolve([]); }
    };
    xhr.onerror = () => reject(new Error('Upload failed — the connection dropped.'));
    xhr.send(form);
  });
}
picker.onchange = () => {
  const n = (picker.files || []).length;
  if (n === 1) setStatus('1 file selected — ready to upload.', true);
  else if (n > 1) setStatus(n + ' files selected — ready to upload.', true);
};
send.onclick = async () => {
  const files = Array.from(picker.files || []);
  if (files.length === 0) { setStatus('Pick at least one file first.'); return; }
  send.disabled = true;
  const totalPlain = files.reduce((sum, f) => sum + f.size, 0);
  let donePlain = 0, ok = 0;
  const failed = [];
  try {
    progress.value = 0;
    const key = decodeKeyFromHash();
    const cipher = await loadCipher();
    for (let i = 0; i < files.length; i++) {
      const file = files[i];
      const label = (i + 1) + '/' + files.length;
      setStatus('Encrypting ' + file.name + ' (' + label + ')...', true);
      const blob = await encryptFile(file, key, cipher, (done) => {
        progress.value = pct(donePlain + done, totalPlain);
      });
      setStatus('Uploading ' + file.name + ' (' + label + ')...', true);
      try {
        const results = await postFile(blob, file, (loaded, total) => {
          const share = total > 0 ? (loaded / total) * file.size : 0;
          progress.value = pct(donePlain + share, totalPlain);
        });
        const bad = results.find(r => r && r.status !== 'done');
        if (bad) failed.push(file.name + ': ' + (bad.error || 'rejected'));
        else ok++;
      } catch (e) {
        failed.push(file.name + ': ' + (e && e.message ? e.message : 'upload failed'));
      }
      donePlain += file.size;
      progress.value = pct(donePlain, totalPlain);
    }
    // Tell the PC the batch is over: without this it cannot tell a finished
    // batch from the gap between two files.
    try { await fetch('/r/' + TOKEN + '/complete', { method: 'POST' }); } catch (e) {}
    if (failed.length === 0) {
      setStatus(ok + (ok === 1 ? ' file uploaded. You can close this page now.' : ' files uploaded. You can close this page now.'), false, 'ok');
    } else {
      setStatus(ok + ' uploaded, ' + failed.length + ' failed:\n' + failed.join('\n'), false, 'error');
    }
  } catch (e) {
    setStatus(e && e.message ? e.message : 'Upload failed.', false, 'error');
  } finally {
    send.disabled = false;
  }
};"##;

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

    /// Two different files that share a name, uploaded in one batch: each part
    /// must be checked against its OWN manifest entry. A name-keyed manifest
    /// handed both parts the last size and rejected one as incomplete.
    #[tokio::test]
    async fn upload_batch_pairs_files_with_manifest_by_position() {
        let (st, reg) = state();
        let s = reg.create_receive_session();
        let first = payload(1200, 5);
        let second = payload(3600, 6);
        let framed_first = aerlynk_crypto::encrypt_all(&s.key, &s.base_nonce, &first);
        let framed_second = aerlynk_crypto::encrypt_all(&s.key, &s.base_nonce, &second);

        let boundary = "b9";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"manifest\"\r\n\r\n");
        let manifest = serde_json::json!([
            { "name": "photo.jpg", "size": 1200 },
            { "name": "photo.jpg", "size": 3600 }
        ])
        .to_string();
        body.extend_from_slice(manifest.as_bytes());
        body.extend_from_slice(b"\r\n");
        for framed in [&framed_first, &framed_second] {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(
                b"Content-Disposition: form-data; name=\"files\"; filename=\"photo.jpg\"\r\n",
            );
            body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
            body.extend_from_slice(framed);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        let app = app(st.clone());
        use tower::ServiceExt;
        let resp = app
            .oneshot(upload_request(&s.token, body, boundary))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json: serde_json::Value = serde_json::from_slice(&collect(resp).await).unwrap();
        assert_eq!(json[0]["status"], "done", "{json}");
        assert_eq!(json[1]["status"], "done", "{json}");
        // Collision suffixing keeps both, so neither was dropped as incomplete.
        assert_eq!(
            std::fs::read(st.downloads_dir.join("photo.jpg")).unwrap(),
            first
        );
        assert_eq!(
            std::fs::read(st.downloads_dir.join("photo (1).jpg")).unwrap(),
            second
        );
    }

    /// The phone reports the end of its upload batch; that marker is what lets
    /// the desktop finish the session instead of ending it between two files.
    #[tokio::test]
    async fn upload_complete_marker_is_recorded_for_the_session() {
        let (st, reg) = state();
        let s = reg.create_receive_session();
        assert!(!s.is_upload_complete());
        let app = app(st);
        use tower::ServiceExt;
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(format!("/r/{}/complete", s.token))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert!(s.is_upload_complete());

        // Unknown tokens stay indistinguishable here too.
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/r/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA/complete")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
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

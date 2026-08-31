// Phone-runtime regression check for the WASM cipher ABI.
//
// Simulates the exact staging pattern the phone pages use
// (src-tauri/src/server/mod.rs, wasm_crypto_runtime): all staging memory comes
// from the module's own allocator (aerlynk_alloc / aerlynk_free) and views are
// created fresh at every use, because the module's allocator may grow linear
// memory during a call — growing a WebAssembly.Memory detaches any previously
// created ArrayBuffer.
//
// Regression guard for: "Cannot perform %TypedArray%.prototype.slice on a
// detached or out-of-bounds ArrayBuffer" (fixed-offset staging + cached views)
// and the dlmalloc heap corruption the fixed offsets caused with 1 MiB chunks.
//
// Run:  node scripts/wasm-runtime-sim.mjs
// (requires target-wasm/wasm32-unknown-unknown/release/aerlynk_crypto.wasm)

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const wasmPath = join(root, "target-wasm", "wasm32-unknown-unknown", "release", "aerlynk_crypto.wasm");
const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {});
const e = instance.exports;

for (const name of ["memory", "aerlynk_alloc", "aerlynk_free", "aerlynk_encrypt", "aerlynk_decrypt", "aerlynk_encrypt_len", "aerlynk_decrypt_len"]) {
  if (!e[name]) { console.error(`FAIL  missing export: ${name}`); process.exit(1); }
}

// Mirror of the phone runtime (wasm_crypto_runtime).
const alloc = len => { const p = e.aerlynk_alloc(len); if (!p) throw new Error("Out of cipher memory."); return p; };
const view = () => new Uint8Array(e.memory.buffer);
function encryptChunk(key, nonce, idx, plain) {
  const outLen = e.aerlynk_encrypt_len(plain.length);
  const keyPtr = alloc(key.length), noncePtr = alloc(nonce.length);
  const plainPtr = alloc(plain.length), outPtr = alloc(outLen);
  try {
    view().set(key, keyPtr); view().set(nonce, noncePtr); view().set(plain, plainPtr);
    const rc = e.aerlynk_encrypt(keyPtr, key.length, noncePtr, nonce.length, idx >>> 0, Math.floor(idx / 4294967296), plainPtr, plain.length, outPtr);
    if (rc !== 0) throw new Error("Encryption failed.");
    return view().slice(outPtr, outPtr + outLen);
  } finally {
    e.aerlynk_free(keyPtr, key.length); e.aerlynk_free(noncePtr, nonce.length);
    e.aerlynk_free(plainPtr, plain.length); e.aerlynk_free(outPtr, outLen);
  }
}
function decryptChunk(key, nonce, idx, framed) {
  const outLen = e.aerlynk_decrypt_len(framed.length);
  if (!outLen) throw new Error("Malformed encrypted frame.");
  const keyPtr = alloc(key.length), noncePtr = alloc(nonce.length);
  const framePtr = alloc(framed.length), outPtr = alloc(outLen);
  try {
    view().set(key, keyPtr); view().set(nonce, noncePtr); view().set(framed, framePtr);
    const rc = e.aerlynk_decrypt(keyPtr, key.length, noncePtr, nonce.length, idx >>> 0, Math.floor(idx / 4294967296), framePtr + 4, framed.length - 4, outPtr);
    if (rc !== 0) throw new Error("Decryption failed.");
    return view().slice(outPtr, outPtr + outLen);
  } finally {
    e.aerlynk_free(keyPtr, key.length); e.aerlynk_free(noncePtr, nonce.length);
    e.aerlynk_free(framePtr, framed.length); e.aerlynk_free(outPtr, outLen);
  }
}

const KEY = 32, NONCE = 12, CHUNK = 1024 * 1024;
const key = new Uint8Array(KEY).fill(7);
const nonce = new Uint8Array(NONCE).fill(3);
const bytes = (n, seed) => { const v = new Uint8Array(n); let x = BigInt(seed); for (let i = 0; i < n; i++) { x = (x * 1103515245n + 12345n) & 0xffffffffn; v[i] = Number(x >> 16n) & 0xff; } return v; };

let failures = 0;
const check = (name, cond) => { console.log(`${cond ? "PASS" : "FAIL"}  ${name}`); if (!cond) failures++; };

// Multi-chunk round trip at the real phone chunk size, plus a partial tail.
const sizes = [CHUNK, CHUNK, CHUNK, 777, 65536];
let framedParts = [];
sizes.forEach((size, idx) => {
  const plain = bytes(size, idx + 1);
  const framed = encryptChunk(key, nonce, idx, plain);
  check(`chunk ${idx}: framed length = ${size} + 4 + 16`, framed.length === size + 4 + 16);
  framedParts.push({ framed, plain });
});

framedParts.forEach(({ framed, plain }, idx) => {
  const round = decryptChunk(key, nonce, idx, framed);
  check(`chunk ${idx}: decrypt round-trips (${plain.length} bytes)`, round.length === plain.length && round.every((b, i) => b === plain[i]));
});

// Tamper detection still works through the new staging path.
{
  const tampered = framedParts[0].framed.slice();
  tampered[10] ^= 0x01;
  let threw = false;
  try { decryptChunk(key, nonce, 0, tampered); } catch { threw = true; }
  check("tampered chunk rejected through new staging", threw);
}

console.log(failures === 0 ? "\nALL RUNTIME-SIM CHECKS PASSED" : `\n${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);

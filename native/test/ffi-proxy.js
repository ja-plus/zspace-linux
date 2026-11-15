// M2 end-to-end: encrypt a service config exactly like background.js encodeService
// (AES-256-GCM, key/iv from MT19937(4728423), ct||tag -> base64), call
// libzrpc_start_service through ffi-napi, then verify the local proxy forwards
// to a mock upstream. Fully offline — no NAS involved.
const ffi = require('@lwahonen/ffi-napi');
const crypto = require('crypto');
const http = require('http');
const path = require('path');
const assert = require('assert');

// Verified deterministic key (M0): MT19937(4728423)x64 -> SHA256 hex -> [0:32]/[0:16].
const KEY = Buffer.from('3891346e92151849d58e70de02a05c59', 'utf8');
const IV = Buffer.from('3891346e92151849', 'utf8');

function encodeService(obj) {
  const c = crypto.createCipheriv('aes-256-gcm', KEY, IV);
  const ct = Buffer.concat([c.update(JSON.stringify(obj), 'utf8'), c.final()]);
  return Buffer.concat([ct, c.getAuthTag()]).toString('base64');
}

const SO = path.resolve(__dirname, '../libzrpc/target/release/libzrpc.so');
const lib = ffi.Library(SO, {
  libzrpc_init: ['int', ['string']],
  libzrpc_uninit: ['void', []],
  libzrpc_start_service: ['string', ['string']],
  libzrpc_stop_all_service: ['void', []],
});

async function main() {
  // mock NAS upstream (HTTP for offline test; real NAS is HTTPS)
  const upstream = http.createServer((req, res) => {
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ ok: true, path: req.url }));
  });
  await new Promise((r) => upstream.listen(0, '127.0.0.1', r));
  const uport = upstream.address().port;

  // free local port for the proxy
  const probe = http.createServer();
  await new Promise((r) => probe.listen(0, '127.0.0.1', r));
  const lport = probe.address().port;
  await new Promise((r) => probe.close(r));

  lib.libzrpc_init(JSON.stringify({ log: { filepath: '/tmp/zrpc-m2.log' }, env: { data_dir: '/tmp/zrpc-data' } }));

  const cfg = { local_port: lport, remote_port: uport, type: 1, nas_id: '127.0.0.1', scheme: 'http', ts: Date.now() };
  const result = JSON.parse(lib.libzrpc_start_service(encodeService(cfg)));
  console.log('start_service ->', result);
  assert.strictEqual(result.code, 0, 'start_service succeeds');

  // give the proxy thread a moment to bind
  await new Promise((r) => setTimeout(r, 200));

  const resp = await fetch(`http://127.0.0.1:${lport}/system/recent`);
  const body = await resp.json();
  console.log('proxied response ->', body);
  assert.strictEqual(resp.status, 200);
  assert.strictEqual(body.ok, true);
  assert.ok(body.path.includes('webagent=v2'), 'proxy adds webagent=v2');
  assert.ok(body.path.includes('rnd='), 'proxy adds rnd cache-buster');

  lib.libzrpc_stop_all_service();
  lib.libzrpc_uninit();
  upstream.close();
  console.log('\nOK: start_service decrypt -> proxy -> upstream verified end-to-end (M2 passed)');
}

main().catch((e) => { console.error(e); process.exit(1); });
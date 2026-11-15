// Reproduce the app's encodeService EXACTLY (J-derived de/he, aes-256-gcm,
// ct||tag -> base64) and feed it to our shim, to isolate the decrypt failure.
const fs = require('fs');
const crypto = require('crypto');
const path = require('path');
const ffi = require('@lwahonen/ffi-napi');

const src = fs.readFileSync(path.resolve(__dirname, '../work/app/extracted/background.js'), 'utf8');
const start = src.indexOf('class J{');
const end = src.indexOf('var Z=n(9)', start);
eval(src.slice(start, end) + ';globalThis.J=J;');

// scheme1 key/iv exactly as background.js
let e = 4728423, t = '', n = new J(e);
for (let o = 0; o < 64; o++) { e = n.genrand_int32(); t += e; }
const hex = crypto.createHash('sha256').update(t).digest('hex');
const de = hex.substring(0, 32), he = hex.substring(0, 16);
console.log('de(key):', de, 'he(iv):', he);

// encodeService replica
function encodeService(obj) {
  obj.ts = Date.now();
  const plain = JSON.stringify(obj);
  const c = crypto.createCipheriv('aes-256-gcm', de, he);
  let o = c.update(plain, 'utf8');
  o = Buffer.concat([o, c.final()]);
  // NOTE: app does o.toString('base64') WITHOUT appending getAuthTag here?
  // background.js: o=concat([update,final]); r=o.toString('base64')  -> NO tag appended!
  return o.toString('base64');
}

const SO = path.resolve(__dirname, '../libzrpc/target/release/libzrpc.so');
const lib = ffi.Library(SO, {
  libzrpc_init: ['int', ['string']],
  libzrpc_start_service: ['string', ['string']],
  libzrpc_uninit: ['void', []],
});
lib.libzrpc_init(JSON.stringify({ log: { filepath: '/tmp/zrpc-appenc.log' }, env: { data_dir: '/tmp' } }));

const conf = { local_port: 13579, remote_port: 5055, type: 1, nas_id: 'Z0220022QBRF4' };
const enc = encodeService(conf);
console.log('encoded (no tag):', enc.slice(0, 60), '... len', enc.length);
console.log('start_service ->', lib.libzrpc_start_service(enc));

// also try WITH tag appended (ct||tag)
const c2 = crypto.createCipheriv('aes-256-gcm', de, he);
let o2 = c2.update(JSON.stringify({ ...conf, ts: Date.now() }), 'utf8');
o2 = Buffer.concat([o2, c2.final(), c2.getAuthTag()]);
console.log('start_service (with tag) ->', lib.libzrpc_start_service(o2.toString('base64')));
lib.libzrpc_uninit();
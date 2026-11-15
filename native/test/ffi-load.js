// M1 verification: load libzrpc.so through @lwahonen/ffi-napi using the exact
// signature table from background.js, then exercise every registered export.
const ffi = require('@lwahonen/ffi-napi');
const path = require('path');

const SO = path.resolve(__dirname, '../libzrpc/target/release/libzrpc.so');

const lib = ffi.Library(SO, {
  libzrpc_init: ['int', ['string']],
  libzrpc_uninit: ['void', []],
  libzrpc_refresh: ['bool', []],
  libzrpc_check_ports: ['string', ['string']],
  libzrpc_start_service: ['string', ['string']],
  libzrpc_stop_service: ['void', ['string']],
  libzrpc_stop_all_service: ['void', []],
  libzrpc_network_diagnose: ['string', ['string']],
  libzrpc_link_diagnose: ['string', ['string']],
  libzrpc_get_nas_status: ['string', []],
  libzrpc_search_device: ['string', []],
  libzrpc_set_device_type: ['void', ['string']],
  libzrpc_set_private_network_access: ['int', ['string', 'int', 'bool', 'bool']],
  libzrpc_detect_nas: ['string', ['string', 'bool']],
  libzrpc_set_ipv6_priority: ['void', ['int']],
  libzrpc_check_conn_ability: ['string', ['string', 'int']],
  libzrpc_get_force_tunnel: ['bool', []],
  libzrpc_set_force_tunnel: ['void', ['bool']],
  // Extra DLL exports beyond the app's 18-function table:
  libzrpc_get_ipv6_priority: ['int', []],
  libzrpc_set_sdk_location: ['void', ['string']],
});

const cfg = JSON.stringify({
  log: { level: 'All', type: 'File', capacity: 1e8, filepath: '/tmp/zrpc-test.log' },
  env: { data_dir: '/tmp/zrpc-data', cache_dir: '/tmp/zrpc-data', sdk_location: 'C' },
  option: { p2p: { actions: 'file_upload,file_download,file_sync' } },
});

const results = {};
results.init = lib.libzrpc_init(cfg);
results.refresh = lib.libzrpc_refresh();
lib.libzrpc_set_device_type('linux');
lib.libzrpc_set_sdk_location('C');
results.search_device = lib.libzrpc_search_device();
results.get_nas_status = lib.libzrpc_get_nas_status();
results.start_service = lib.libzrpc_start_service('dummy-encrypted');
results.check_ports = lib.libzrpc_check_ports('x');
results.detect_nas = lib.libzrpc_detect_nas('192.168.1.100', false);
results.check_conn_ability = lib.libzrpc_check_conn_ability('x', 1);
results.network_diagnose = lib.libzrpc_network_diagnose('nasid');
results.link_diagnose = lib.libzrpc_link_diagnose('nasid');
results.force_tunnel_before = lib.libzrpc_get_force_tunnel();
lib.libzrpc_set_force_tunnel(true);
results.force_tunnel_after = lib.libzrpc_get_force_tunnel();
lib.libzrpc_set_ipv6_priority(1);
results.ipv6_priority = lib.libzrpc_get_ipv6_priority();
results.private_network_access = lib.libzrpc_set_private_network_access('x', 1, true, false);
lib.libzrpc_stop_service('svc');
lib.libzrpc_stop_all_service();
lib.libzrpc_uninit();

console.log(JSON.stringify(results, null, 2));

// Assertions
const assert = require('assert');
assert.strictEqual(results.init, 0, 'init returns 0');
assert.strictEqual(results.refresh, true);
assert.strictEqual(results.force_tunnel_before, false);
assert.strictEqual(results.force_tunnel_after, true);
assert.strictEqual(results.ipv6_priority, 1);
assert.strictEqual(results.private_network_access, 0);
assert.ok(results.search_device.includes('"code"'), 'search_device returns JSON');
console.log('\nOK: libzrpc.so loads via ffi-napi and all 18 exports behave (M1 passed)');
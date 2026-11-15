#!/usr/bin/env python3
"""Active LAN discovery probe — sends ONE standard discovery broadcast (exactly
what the official client sends on startup) and decrypts the NAS response with
the recovered key. Benign: read-only identity query, no writes/state change.

Verifies M2 request format + key + response fields against the real NAS.
"""
import socket, time, binascii, base64, json, sys
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

KEY = b'3891346e92151849d58e70de02a05c59'
IV = b'3891346e92151849'
aes = AESGCM(KEY)

def dec(data):
    try:
        return aes.decrypt(IV, data, None)
    except Exception:
        return None

# request = hex( AES-256-GCM( {"key":"tdxnassearch"} ) )
req_json = json.dumps({"key": "tdxnassearch"}, separators=(',', ':')).encode()
req_hex = binascii.hexlify(aes.encrypt(IV, req_json, None))

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
sock.bind(('0.0.0.0', 0))
sock.settimeout(1.0)

targets = ['192.168.5.2', '255.255.255.255', '192.168.111.255']
for t in targets:
    try:
        sock.sendto(req_hex, (t, 5577))
        print(f'sent discovery request ({len(req_hex)} hex bytes) -> {t}:5577', flush=True)
    except Exception as e:
        print(f'send to {t} failed: {e}', flush=True)

print('listening for NAS response (~8s)...', flush=True)
end = time.time() + 8
seen = 0
while time.time() < end:
    try:
        data, addr = sock.recvfrom(65535)
    except socket.timeout:
        continue
    seen += 1
    print(f'\n=== response #{seen} from {addr} len={len(data)} ===', flush=True)
    pt = None
    for label, raw in [('HEX', lambda: binascii.unhexlify(data.strip())),
                       ('RAW', lambda: data),
                       ('B64', lambda: base64.b64decode(data.strip()))]:
        try:
            pt = dec(raw())
            if pt:
                print(f'{label}+AES-GCM ->', pt.decode('utf-8', 'replace'), flush=True)
                break
        except Exception:
            continue
    if not pt:
        print('NOT decrypted; raw[:160] hex =', data[:160].hex(), flush=True)
        print('raw[:160] ascii =', data[:160], flush=True)

print(f'\ndone: {seen} response(s)', flush=True)
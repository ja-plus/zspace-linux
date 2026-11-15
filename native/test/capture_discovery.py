#!/usr/bin/env python3
"""Passive LAN discovery capture — verifies the recovered crypto against REAL
official-client traffic. Binds UDP 5577 and ONLY receives (sends nothing).
Tries hex / raw / base64 framing + AES-256-GCM with the recovered key.

Usage: python3 capture_discovery.py [seconds]
"""
import socket, sys, time, binascii, base64
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

# Recovered deterministic key (M0): MT19937(4728423)x64 -> SHA256 hex -> [0:32]/[0:16]
KEY = b'3891346e92151849d58e70de02a05c59'
IV = b'3891346e92151849'
aes = AESGCM(KEY)

def dec(data):
    try:
        return aes.decrypt(IV, data, None)
    except Exception:
        return None

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
try:
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
except OSError:
    pass
sock.bind(('0.0.0.0', 5577))
sock.settimeout(1.0)
print('passive listen on UDP 5577 (sending nothing)...', flush=True)

duration = int(sys.argv[1]) if len(sys.argv) > 1 else 60
end = time.time() + duration
seen = 0
ok = 0
while time.time() < end:
    try:
        data, addr = sock.recvfrom(65535)
    except socket.timeout:
        continue
    seen += 1
    print(f'\n=== pkt #{seen} from {addr} len={len(data)} ===', flush=True)
    pt = None
    # hex framing (mac encrypt hex-encodes)
    try:
        pt = dec(binascii.unhexlify(data.strip()))
        if pt:
            print('HEX+AES-GCM ->', pt.decode('utf-8', 'replace'), flush=True)
    except Exception:
        pass
    if not pt:
        pt = dec(data)
        if pt:
            print('RAW+AES-GCM ->', pt.decode('utf-8', 'replace'), flush=True)
    if not pt:
        try:
            pt = dec(base64.b64decode(data.strip()))
            if pt:
                print('B64+AES-GCM ->', pt.decode('utf-8', 'replace'), flush=True)
        except Exception:
            pass
    if pt:
        ok += 1
    else:
        print('NOT decrypted; raw[:120] hex =', data[:120].hex(), flush=True)

print(f'\ndone: {seen} packets, {ok} decrypted with recovered key', flush=True)
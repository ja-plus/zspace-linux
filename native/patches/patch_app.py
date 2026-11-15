#!/usr/bin/env python3
"""Patch the extracted official app for native Linux (M3).

Run from the extracted app dir (asar JS + unpacked natives merged):
  1. background.js: load libzrpc.so on linux (was win32 dll fallback).
  2. Remove win32 build/Release/*.node so node-gyp-build picks linux prebuilds.
  3. Install our native libzrpc.so into sdk/.

Idempotent. Reports each change. Does NOT touch the official exe/asar source.
"""
import os, sys, glob, shutil

APP = sys.argv[1] if len(sys.argv) > 1 else '.'
SO_SRC = sys.argv[2] if len(sys.argv) > 2 else None
BG = os.path.join(APP, 'background.js')

OLD = '"darwin"==process.platform?"libzrpc.dylib":"libzrpc_x64.dll"'
NEW = '"darwin"==process.platform?"libzrpc.dylib":"linux"==process.platform?"libzrpc.so":"libzrpc_x64.dll"'

def patch_bg():
    src = open(BG, encoding='utf-8').read()
    if NEW in src:
        print('[bg] linux branch already present'); return
    if OLD not in src:
        print('[bg] WARNING: dll-load pattern not found'); return
    src = src.replace(OLD, NEW)
    open(BG, 'w', encoding='utf-8').write(src)
    print('[bg] patched dll load -> libzrpc.so on linux')

def strip_win32_bindings():
    n = 0
    for f in glob.glob(os.path.join(APP, 'node_modules', '**', 'build', 'Release', '*.node'), recursive=True):
        # win32 PE bindings are invalid on linux; prebuilds/linux-x64 take over
        with open(f, 'rb') as fh:
            magic = fh.read(2)
        if magic == b'MZ':
            os.remove(f); n += 1
    print(f'[bindings] removed {n} win32 .node (linux prebuilds remain)')

def install_so():
    if not SO_SRC:
        print('[so] no source given, skip'); return
    dst_dir = os.path.join(APP, 'sdk')
    os.makedirs(dst_dir, exist_ok=True)
    dst = os.path.join(dst_dir, 'libzrpc.so')
    shutil.copyfile(SO_SRC, dst)
    print(f'[so] installed {dst} ({os.path.getsize(dst)} bytes)')

if __name__ == '__main__':
    patch_bg()
    strip_win32_bindings()
    install_so()
    print('done')
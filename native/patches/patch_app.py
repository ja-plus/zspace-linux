#!/usr/bin/env python3
"""Patch the extracted official app for native Linux (M3 + player + sync + mount).

Usage:
    patch_app.py <app-dir> [libzrpc.so] [zspace-player] [syncthing]
                 [zspace-mount-monitor] [rclone]

  1. background.js: load libzrpc.so on linux (was win32 dll fallback).
  2. background.js: 6 player patches from native/player/PATCH-SPEC.md
     (Linux player path, no VlcPreload.exe, pkill, preloaded flag,
     playsdk disabled, skip Public/ZSpace copy).
  3. background.js: sync patches from native/sync/PATCH-SPEC.md
     (bin path -> bin/syncthing, upstream `generate --home=` syntax,
     accept upstream ISO log timestamps).
  4. background.js: mountV2 patches (drop .exe on linux, kill the monitor by
     its 15-char truncated /proc/comm name).
  5. Remove win32 build/Release/*.node so node-gyp-build picks linux prebuilds.
  6. Install our native libzrpc.so into sdk/.
  7. Install zspace-player into uzplayer/ (0755).
  8. Install syncthing into bin/syncthing (0755).
  9. Install zspace-mount-monitor as bin/ZSpaceMountMonitor, rclone as
     bin/ZSpaceMount, and the official filter.txt into bin/filter.txt.

Idempotent. Validates JS syntax before writing when a JS engine is available
(`ZS_NODE` env, or `node` in PATH; the Electron binary works with
ELECTRON_RUN_AS_NODE=1). Does NOT touch the official exe/asar source.
"""
import glob
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))

APP = sys.argv[1] if len(sys.argv) > 1 else '.'
SO_SRC = sys.argv[2] if len(sys.argv) > 2 and sys.argv[2] else None
PLAYER_SRC = (
    sys.argv[3]
    if len(sys.argv) > 3 and sys.argv[3]
    else os.path.join(REPO, 'native/player/target/release/zspace-player')
)
SYNC_SRC = (
    sys.argv[4]
    if len(sys.argv) > 4 and sys.argv[4]
    else os.path.join(REPO, 'native/work/syncthing/syncthing')
)
MOUNT_SRC = (
    sys.argv[5]
    if len(sys.argv) > 5 and sys.argv[5]
    else os.path.join(REPO, 'native/mount/target/release/zspace-mount-monitor')
)
RCLONE_SRC = (
    sys.argv[6]
    if len(sys.argv) > 6 and sys.argv[6]
    else os.path.join(REPO, 'native/work/rclone/rclone')
)
FILTER_SRC = os.path.join(REPO, 'packaging-native/mount-filter.txt')
BG = os.path.join(APP, 'background.js')

OLD = '"darwin"==process.platform?"libzrpc.dylib":"libzrpc_x64.dll"'
NEW = '"darwin"==process.platform?"libzrpc.dylib":"linux"==process.platform?"libzrpc.so":"libzrpc_x64.dll"'


def syntax_ok(path):
    """Return True/False, or None when no JS engine is available."""
    node = os.environ.get('ZS_NODE') or shutil.which('node')
    if not node:
        return None
    env = dict(os.environ, ELECTRON_RUN_AS_NODE='1')
    try:
        subprocess.run(
            [node, '--check', path],
            check=True,
            capture_output=True,
            env=env,
        )
        return True
    except subprocess.CalledProcessError as e:
        sys.stderr.write((e.stderr or b'').decode('utf-8', 'replace')[:2000])
        return False


def patch_bg():
    src = open(BG, encoding='utf-8').read()
    if NEW in src:
        print('[bg] linux branch already present')
    elif OLD not in src:
        print('[bg] WARNING: dll-load pattern not found')
    else:
        src = src.replace(OLD, NEW)
        open(BG, 'w', encoding='utf-8').write(src)
        print('[bg] patched dll load -> libzrpc.so on linux')


# (name, old, new) — anchors and replacements from PATCH-SPEC.md
PLAYER_PATCHES = [
    ("player-path",
     'za=za.replace("app.asar","app.asar.unpacked");',
     'za=za.replace("app.asar","app.asar.unpacked");"linux"==process.platform&&(za=ka.join(__dirname,"uzplayer/zspace-player"));'),
    ("vlc-preload",
     'Qa=()=>{let e=i.app.getName().toLowerCase()',
     'Qa=()=>{if("linux"==process.platform)return void(global.VlcPreloaded=!0);let e=i.app.getName().toLowerCase()'),
    ("kill-player",
     't=Ra?`killall -9 "${n}"`:`taskkill /f /im "${n}"`',
     't=Ra?`killall -9 "${n}"`:"linux"==process.platform?"pkill -x zspace-player":`taskkill /f /im "${n}"`'),
    ("vlc-preloaded-flag",
     '"darwin"==process.platform?global.VlcPreloaded=!0:Object(Rc.setTimeout)',
     '"darwin"==process.platform||"linux"==process.platform?global.VlcPreloaded=!0:Object(Rc.setTimeout)'),
    ("playsdk-disable",
     ':(Ot=Tt.join(e,"playsdk.dll"),xt=Tt.dirname(Ot),Dt="")});',
     ':"linux"==process.platform?(Ot="",xt="",Dt=""):(Ot=Tt.join(e,"playsdk.dll"),xt=Tt.dirname(Ot),Dt="")});'),
    ("ascii-branch",
     'if(/^[\\000-\\177]*$/.test(ka.dirname(za)))process.emit("vlcPlayerReady",ka.dirname(za)),Qa();else{',
     'if("linux"==process.platform||/^[\\000-\\177]*$/.test(ka.dirname(za)))process.emit("vlcPlayerReady",ka.dirname(za)),Qa();else{'),
]


SYNC_PATCHES = [
    ("sync-bin",
     'this.binPath=U.a.join(__dirname,"bin",nt).replace("app.asar","app.asar.unpacked");',
     'this.binPath=U.a.join(__dirname,"bin",nt).replace("app.asar","app.asar.unpacked");'
     '"linux"==process.platform&&(this.binPath=U.a.join(__dirname,"bin","syncthing"));'),
    ("sync-generate",
     '["-no-browser","-generate="+this.configPath]',
     '"linux"==process.platform?["generate","--home="+this.configPath]:["-no-browser","-generate="+this.configPath]'),
    ("sync-log-date",
     '/^(\\d+\\/\\d+\\/\\d+ \\d+:\\d+:\\d+)/',
     '/^(\\d+[\\/-]\\d+[\\/-]\\d+ \\d+:\\d+:\\d+)/'),
]

# mountV2: monitor/rclone live in <app>/bin. On Linux drop the .exe suffix and
# handle the 15-char /proc/<pid>/comm truncation ("ZSpaceMountMoni").
MOUNT_PATCHES = [
    ("mount-no-exe",
     'vn||(bn+=".exe",wn+=".exe");',
     '"linux"==process.platform||vn||(bn+=".exe",wn+=".exe");'),
    ("mount-kill-comm",
     'e.name.indexOf("MountMonitor")>-1?n=e.pid:e.name.indexOf("Mount")>-1&&(o=e.pid)',
     'e.name.indexOf("MountMonitor")>-1||"linux"==process.platform&&e.name.indexOf("MountMoni")>-1'
     '?n=e.pid:e.name.indexOf("Mount")>-1&&(o=e.pid)'),
]


def apply_patches(tag, patches):
    src = open(BG, encoding='utf-8').read()
    changed = False
    results = []
    for name, old, new in patches:
        if new in src:
            results.append((name, 'already'))
            continue
        count = src.count(old)
        if count == 0:
            results.append((name, 'WARNING: anchor not found (skipped)'))
            continue
        if count > 1:
            results.append((name, f'WARNING: anchor x{count} (skipped)'))
            continue
        src = src.replace(old, new, 1)
        changed = True
        results.append((name, 'patched'))

    if changed:
        # Keep the .js suffix: Node/Electron refuse unknown extensions in
        # `--check` mode.
        fd, tmp = tempfile.mkstemp(dir=APP, prefix='.bgcheck-', suffix='.js')
        os.close(fd)
        try:
            with open(tmp, 'w', encoding='utf-8') as f:
                f.write(src)
            ok = syntax_ok(tmp)
            if ok is False:
                print(f'[{tag}] ERROR: patched JS failed syntax check; not written')
                os.remove(tmp)
                sys.exit(2)
            if ok is None:
                print(f'[{tag}] note: no JS engine for syntax check (ZS_NODE/node)')
            os.replace(tmp, BG)
        finally:
            if os.path.exists(tmp):
                os.remove(tmp)

    for name, state in results:
        print(f'[{tag}] {name}: {state}')
    if any(s.startswith('WARNING') for _, s in results):
        print(f'[{tag}] some anchors were missing; feature may not work (check PATCH-SPEC.md)')


def strip_win32_bindings():
    n = 0
    for f in glob.glob(os.path.join(APP, 'node_modules', '**', 'build', 'Release', '*.node'), recursive=True):
        with open(f, 'rb') as fh:
            magic = fh.read(2)
        if magic == b'MZ':
            os.remove(f)
            n += 1
    print(f'[bindings] removed {n} win32 .node (linux prebuilds remain)')


def install_so():
    if not SO_SRC:
        print('[so] no source given, skip')
        return
    dst_dir = os.path.join(APP, 'sdk')
    os.makedirs(dst_dir, exist_ok=True)
    dst = os.path.join(dst_dir, 'libzrpc.so')
    if os.path.abspath(SO_SRC) != os.path.abspath(dst):
        shutil.copyfile(SO_SRC, dst)
    print(f'[so] installed {dst} ({os.path.getsize(dst)} bytes)')


def install_player():
    if not PLAYER_SRC or not os.path.isfile(PLAYER_SRC):
        print(f'[player] ERROR: player binary not found: {PLAYER_SRC}')
        print('[player] build it first: cd native/player && cargo build --release')
        sys.exit(3)
    dst_dir = os.path.join(APP, 'uzplayer')
    os.makedirs(dst_dir, exist_ok=True)
    dst = os.path.join(dst_dir, 'zspace-player')
    if os.path.abspath(PLAYER_SRC) != os.path.abspath(dst):
        shutil.copyfile(PLAYER_SRC, dst)
    os.chmod(dst, 0o755)
    print(f'[player] installed {dst} ({os.path.getsize(dst)} bytes)')


def install_sync():
    if not SYNC_SRC or not os.path.isfile(SYNC_SRC):
        print(f'[sync] WARNING: syncthing binary not found: {SYNC_SRC}')
        print('[sync] download 1.27.7 into native/work/syncthing/syncthing or pass arg 4')
        return
    dst_dir = os.path.join(APP, 'bin')
    os.makedirs(dst_dir, exist_ok=True)
    dst = os.path.join(dst_dir, 'syncthing')
    if os.path.abspath(SYNC_SRC) != os.path.abspath(dst):
        shutil.copyfile(SYNC_SRC, dst)
    os.chmod(dst, 0o755)
    print(f'[sync] installed {dst} ({os.path.getsize(dst)} bytes)')


def install_mount():
    bin_dir = os.path.join(APP, 'bin')
    os.makedirs(bin_dir, exist_ok=True)
    pairs = [
        ('ZSpaceMountMonitor', MOUNT_SRC, 0o755),
        ('ZSpaceMount', RCLONE_SRC, 0o755),
        ('filter.txt', FILTER_SRC, 0o644),
    ]
    for name, src, mode in pairs:
        if not src or not os.path.isfile(src):
            print(f'[mount] WARNING: missing {name} source: {src}')
            continue
        dst = os.path.join(bin_dir, name)
        if os.path.abspath(src) != os.path.abspath(dst):
            shutil.copyfile(src, dst)
        os.chmod(dst, mode)
        print(f'[mount] installed {dst} ({os.path.getsize(dst)} bytes)')


if __name__ == '__main__':
    patch_bg()
    apply_patches('player', PLAYER_PATCHES)
    apply_patches('sync', SYNC_PATCHES)
    apply_patches('mount', MOUNT_PATCHES)
    strip_win32_bindings()
    install_so()
    install_player()
    install_sync()
    install_mount()
    print('done')

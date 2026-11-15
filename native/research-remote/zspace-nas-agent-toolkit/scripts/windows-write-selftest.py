#!/usr/bin/env python3
"""W1/V24/V33 native process acceptance. No NAS, OAuth or existing profile used.
Usage: python scripts/windows-write-selftest.py path/to/zspace-cli.exe
"""
import concurrent.futures
import http.server
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import threading
import urllib.parse

PAYLOAD = b'0123456789abcdefghijklmnopqrstuvwxyz'
CANARIES = ('synthetic-write-access-canary', 'synthetic-write-refresh-canary')

class Fixture(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_HEAD(self):
        self.send_response(200)
        self.send_header('Content-Length', str(len(PAYLOAD)))
        self.end_headers()

    def do_GET(self):
        path = urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query).get('path', [''])[0]
        if path == '/truncated':
            self.send_response(200)
            self.send_header('Content-Length', str(len(PAYLOAD)))
            self.end_headers()
            self.wfile.write(b'bad')
            self.close_connection = True
            return
        self.send_response(200)
        self.send_header('Content-Type', 'application/octet-stream')
        self.send_header('Content-Length', str(len(PAYLOAD)))
        self.send_header('ETag', '"fixture-v1"')
        self.end_headers()
        self.wfile.write(PAYLOAD)


def main():
    binary = str(pathlib.Path(sys.argv[1]).resolve())
    checks = []
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Fixture)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    host = 'http://127.0.0.1:' + str(server.server_port)
    try:
        with tempfile.TemporaryDirectory(prefix='zspace-write-native-') as tmp:
            root = pathlib.Path(tmp)
            profile = root / 'home/profiles/write-test'
            profile.mkdir(parents=True, mode=0o700)
            for folder in (root / 'home', root / 'home/profiles', profile):
                folder.chmod(0o700)
            def private(name, value):
                fd = os.open(str(profile / name), os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
                with os.fdopen(fd, 'w', encoding='utf-8') as f:
                    json.dump(value, f)
            private('credentials.json', dict(access_token=CANARIES[0], refresh_token=CANARIES[1], nas_id='synthetic-nas', expires_at=9999999999))
            private('session-authority.json', dict(backend='file', session_id='synthetic-write-session', cleanup_pending=False))
            env = dict(os.environ, ZSPACE_CLI_HOME=str(root / 'home'))
            base = [binary, '--profile', 'write-test', '--credential-store', 'file']
            def run(args, expected=None):
                r = subprocess.run(base + args, env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True, encoding='utf-8', timeout=30)
                assert not any(s in r.stdout + r.stderr for s in CANARIES), 'synthetic secret leaked'
                if expected:
                    assert r.returncode != 0, 'expected failure: ' + str(args)
                    data = json.loads(r.stderr)
                    assert data['error']['code'] == expected, data
                else:
                    assert r.returncode == 0, r.stderr
                    data = json.loads(r.stdout)
                return data
            def clean():
                assert not list(root.rglob('.zspace-*.tmp')), 'temporary files left behind'
            def download(folder, local, remote='/fixture', **params):
                args = ['--host', host, '--allow-insecure-http', '--local-root', str(folder), 'file', 'download', '--params', json.dumps(dict(path=remote, local_path=local, **params)), '--yes']
                return args
            for _ in range(10):
                run(['--host', host, '--allow-insecure-http', 'status'])
                assert json.loads((profile / 'config.json').read_text(encoding='utf-8'))['host'] == host
                run(['--clear-host', 'status'])
                assert json.loads((profile / 'config.json').read_text(encoding='utf-8')).get('host') is None
            checks.append('host set/clear x10')
            run(['--host', 'http://127.0.0.1', '--allow-insecure-http', 'status'])
            assert json.loads((profile / 'config.json').read_text(encoding='utf-8'))['host'] == 'http://127.0.0.1:80'
            run(['--clear-host', 'status'])
            checks.append('host implicit port normalized to 80')
            for mode in ('cloud', 'auto'):
                run(['--mode', mode, 'status'])
            checks.append('mode configuration install')
            for label in ('A', 'B', 'C'):
                folder = root / label
                folder.mkdir()
                for n in range(3):
                    run(download(folder, 'new' + str(n) + '.bin'))
                    assert (folder / ('new' + str(n) + '.bin')).read_bytes() == PAYLOAD
            checks.append('new target downloads 3 directories x3')
            folder = root / 'A'
            target = folder / 'existing.bin'
            target.write_bytes(b'old')
            run(download(folder, target.name), 'E_EXISTS')
            assert target.read_bytes() == b'old'
            run(download(folder, target.name, overwrite=True))
            assert target.read_bytes() == PAYLOAD
            checks.append('existing target protected; explicit overwrite matches bytes')
            target.write_bytes(PAYLOAD[:10])
            run(download(folder, target.name, offset=10, overwrite=True))
            assert target.read_bytes() == PAYLOAD
            checks.append('offset matching prefix installs complete content')
            target.write_bytes(b'XXXXXXXXXX')
            run(download(folder, target.name, offset=10, overwrite=True), 'E_CONFLICT')
            assert target.read_bytes() == b'XXXXXXXXXX'
            clean()
            checks.append('offset mismatch preserves old target and removes temporary')
            for filename, overwrite in ((target.name, True), ('absent.bin', False)):
                args = download(folder, filename, remote='/truncated', overwrite=overwrite)
                r = subprocess.run(base + args, env=env, capture_output=True, text=True, encoding='utf-8', timeout=30)
                assert r.returncode != 0 and json.loads(r.stderr)['ok'] is False
                assert not any(s in r.stdout + r.stderr for s in CANARIES)
                assert target.read_bytes() == b'XXXXXXXXXX'
                assert not (folder / 'absent.bin').exists()
                clean()
            checks.append('failed transfer preserves target and removes empty temporary')
            def writer(_):
                for n in range(10):
                    run(['--host', host, '--allow-insecure-http', 'status'] if n % 2 else ['--clear-host', 'status'])
            with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
                list(pool.map(writer, range(4)))
            run(['--clear-host', 'status'])
            clean()
            checks.append('4 concurrent processes x10 profile writes')
            run(['logout'])
            assert run(['status'])['data']['logged_in'] is False
            checks.append('isolated profile logout')
        version = subprocess.check_output([binary, '--version'], text=True).strip()
        print(json.dumps(dict(binary_version=version, platform=sys.platform, passed=len(checks), checks=checks, real_NAS_used=False, real_credentials_used=False, temporary_fixture_removed=True), ensure_ascii=False, indent=2))
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)

if __name__ == '__main__':
    main()

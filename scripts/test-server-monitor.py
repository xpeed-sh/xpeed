"""Local-only SSH transport and optional Linux sample checks for the monitor."""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import threading
import time

root = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(root / '.tools' / 'ssh-testdeps'))
import paramiko

parser = argparse.ArgumentParser()
parser.add_argument('--host', required=True)
parser.add_argument('--extension', required=True)
parser.add_argument('--samples', help='Optional output of the collector on real local Linux')
args = parser.parse_args()

def fixture(second=False):
    fields = ['0'] * 22
    fields[0], fields[11], fields[19], fields[21] = 'R', '30' if second else '10', '1', '100'
    return ('CPU\n' + ('cpu 20 0 30 150 0 0 0 0\n' if second else 'cpu 10 0 20 70 0 0 0 0\n')
            + 'MEMORY\nMemTotal: 1024 kB\nMemAvailable: 512 kB\nUPTIME\n'
            + ('4 0\n' if second else '2 0\n')
            + 'NETWORK\neth0: 100 0 0 0 0 0 0 0 100 0 0 0 0 0 0 0\n'
            + 'DISK\n/dev/vda 1024 512 512 50% /\nPAGESIZE\n4096\nPROCESSES\n123 (worker) '
            + ' '.join(fields) + '\n')

samples = [fixture(), fixture(True), fixture(True)]
if args.samples:
    text = Path(args.samples).read_text(encoding='utf-8')
    samples = [block.split('END\n', 1)[0] for block in text.split('BEGIN\n')[1:] if 'END\n' in block]
    assert len(samples) >= 3

# Full JSON-RPC exchange with the packaged adapter, including a late previous-server sample.
messages = [dict(jsonrpc='2.0', id=1, method='initialize', params={}),
            dict(jsonrpc='2.0', method='server/context', params=dict(id=1, name='Linux local', host='localhost', port=22, observing=True))]
for sample in samples:
    messages.append(dict(jsonrpc='2.0', method='server/sample', params=dict(contextId=1, sample=sample)))
messages += [dict(jsonrpc='2.0', id=2, method='views/read', params=dict(viewId='monitor')),
             dict(jsonrpc='2.0', method='server/context', params=dict(id=2, name='Server B', host='b', port=22, observing=True)),
             dict(jsonrpc='2.0', method='server/sample', params=dict(contextId=1, sample=samples[-1])),
             dict(jsonrpc='2.0', id=3, method='views/read', params=dict(viewId='monitor')),
             dict(jsonrpc='2.0', id=4, method='shutdown', params={})]
result = subprocess.run([str(root / args.extension)], input=''.join(json.dumps(m) + '\n' for m in messages), encoding='utf-8', capture_output=True, timeout=10)
assert result.returncode == 0, result.stderr
responses = [json.loads(line) for line in result.stdout.splitlines()]
data = next(r['result']['dashboard'] for r in responses if r.get('id') == 2)
assert len(data['metrics']) >= 3, data
assert data['processes'], data
assert data['metrics'][0]['history'], data
stale = next(r['result']['dashboard'] for r in responses if r.get('id') == 3)
assert stale['contextId'] == 2 and stale['metrics'] == []
folder = root / 'artifacts' / 'monitor-test'
folder.mkdir(exist_ok=True)
(folder / 'dashboard.json').write_text(json.dumps(data, ensure_ascii=False), encoding='utf-8')

# Exercise real OpenSSH + host Askpass against one ephemeral local SSH server.
stop = threading.Event()
key = paramiko.RSAKey.generate(2048)
listener = socket.socket()
listener.bind(('127.0.0.1', 0))
listener.listen(5)
listener.settimeout(.2)
port = listener.getsockname()[1]
hosts = folder / 'known_hosts'
hosts.write_text(f'[127.0.0.1]:{port} {key.get_name()} {key.get_base64()}\n', encoding='ascii')
config = folder / 'connection.json'
config.write_text(json.dumps(dict(host='127.0.0.1', port=port, user='monitor-test', identity='')), encoding='ascii')
password = 'Local-monitor-test-ONLY!'
counts = dict(auth=0, exec=0, samples=0, closed=0)
transports = []

class Server(paramiko.ServerInterface):
    def get_allowed_auths(self, user): return 'password'
    def check_auth_password(self, user, value):
        if user == 'monitor-test' and value == password:
            counts['auth'] += 1
            return paramiko.AUTH_SUCCESSFUL
        return paramiko.AUTH_FAILED
    def check_channel_request(self, kind, chanid):
        return paramiko.OPEN_SUCCEEDED if kind == 'session' else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED
    def check_channel_exec_request(self, channel, command):
        assert command == b'sh -s'
        counts['exec'] += 1
        def stream():
            source = b''
            while True:
                packet = channel.recv(4096)
                if not packet: break
                source += packet
            assert b'/proc/stat' in source and b'sleep 3' in source
            try:
                index = 0
                while not stop.is_set():
                    sample = samples[min(index, len(samples)-1)]
                    channel.sendall(('BEGIN\n' + sample + 'END\n').encode('utf-8'))
                    counts['samples'] += 1
                    index += 1
                    time.sleep(.2)
            except (OSError, EOFError): pass
            finally:
                counts['closed'] += 1
        threading.Thread(target=stream, daemon=True).start()
        return True

def serve():
    while not stop.is_set():
        try: client, _ = listener.accept()
        except socket.timeout: continue
        except OSError: break
        transport = paramiko.Transport(client)
        transports.append(transport)
        transport.add_server_key(key)
        threading.Thread(target=lambda t=transport: t.start_server(server=Server()), daemon=True).start()
threading.Thread(target=serve, daemon=True).start()
try:
    result = subprocess.run([str(root / args.host), '--monitor-smoke-test', str(config), str(hosts)], env=dict(os.environ, XPEED_TEST_PASSWORD=password), capture_output=True, timeout=20)
    assert result.returncode == 0, result.stderr.decode(errors='replace')
    deadline = time.time() + 2
    while counts['closed'] == 0 and time.time() < deadline: time.sleep(.05)
    assert counts['auth'] == 1 and counts['exec'] == 1 and counts['samples'] >= 3 and counts['closed'] == 1, counts
    print('Monitor: JSON-RPC, graphs, process data, server switch and SSH cancellation passed.', counts)
finally:
    stop.set()
    listener.close()
    for transport in transports: transport.close()

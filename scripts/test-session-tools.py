"""Real OpenSSH/SFTP tests on two localhost servers and temporary files only."""
import json, logging, os, socket, subprocess, sys, tempfile, threading, time
from pathlib import Path
root = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(root / '.tools/ssh-testdeps'))
import paramiko
logging.getLogger('paramiko').setLevel(logging.CRITICAL)
shell = Path('C:/msys64/usr/bin/sh.exe') if os.name == 'nt' else Path('/bin/sh')

class Files(paramiko.SFTPServerInterface):
    def __init__(self, server, **kwargs): super().__init__(server, **kwargs); self.root = server.root
    def local(self, path):
        path = (self.root / path.removeprefix('/test').lstrip('/')).resolve()
        if not path.is_relative_to(self.root): raise PermissionError('Outside fixture')
        return path
    def canonicalize(self, path): return '/test/' + self.local(path).relative_to(self.root).as_posix()
    def list_folder(self, path):
        try:
            result = []
            for item in self.local(path).iterdir():
                attr = paramiko.SFTPAttributes.from_stat(item.stat()); attr.filename = item.name; result.append(attr)
            return result
        except OSError as e: return paramiko.SFTPServer.convert_errno(e.errno)
    def stat(self, path):
        try: return paramiko.SFTPAttributes.from_stat(self.local(path).stat())
        except OSError as e: return paramiko.SFTPServer.convert_errno(e.errno)
    lstat = stat
    def open(self, path, flags, attr):
        try:
            fd = os.open(self.local(path), flags | getattr(os, 'O_BINARY', 0), attr.st_mode or 0o600)
            mode = 'rb' if flags & os.O_RDWR == 0 and flags & os.O_WRONLY == 0 else 'r+b'
            file = os.fdopen(fd, mode); handle = paramiko.SFTPHandle(flags)
            if mode == 'rb': handle.readfile = file
            else: handle.writefile = file
            return handle
        except OSError as e: return paramiko.SFTPServer.convert_errno(e.errno)
    def remove(self, path):
        try: self.local(path).unlink(); return paramiko.SFTP_OK
        except OSError as e: return paramiko.SFTPServer.convert_errno(e.errno)
    def rename(self, old, new):
        try:
            if self.local(new).exists(): return paramiko.SFTP_FAILURE
            self.local(old).rename(self.local(new)); return paramiko.SFTP_OK
        except OSError as e: return paramiko.SFTPServer.convert_errno(e.errno)
    def posix_rename(self, old, new):
        try: os.replace(self.local(old), self.local(new)); return paramiko.SFTP_OK
        except OSError as e: return paramiko.SFTPServer.convert_errno(e.errno)

class Server(paramiko.ServerInterface):
    def __init__(self, folder, public_key): self.root = folder; self.public_key = public_key
    def check_auth_publickey(self, username, key): return paramiko.AUTH_SUCCESSFUL if username == 'xpeed-test' and key == self.public_key else paramiko.AUTH_FAILED
    def get_allowed_auths(self, username): return 'publickey'
    def check_channel_request(self, kind, channel_id): return paramiko.OPEN_SUCCEEDED if kind == 'session' else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED
    def check_channel_pty_request(self, channel, term, width, height, pixelwidth, pixelheight, modes): return True
    def check_channel_shell_request(self, channel):
        def run():
            env=dict(os.environ, PS1='xpeed-test@fixture:/test$ ', PATH='/usr/bin:/bin')
            process=subprocess.Popen([str(shell),'--noprofile','--norc','-i'],cwd=self.root,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
            def output():
                try:
                    while chunk := os.read(process.stdout.fileno(),4096): channel.sendall(chunk)
                except (EOFError,OSError): pass
            threading.Thread(target=output,daemon=True).start()
            try:
                pending=''
                while chunk := channel.recv(32768):
                    pending+=chunk.decode()
                    if '\x03' in pending:
                        process.terminate(); break
                    while '\r' in pending or '\n' in pending:
                        parts=pending.replace('\r','\n').split('\n',1)
                        text,pending=parts
                        text=text.replace("cd -- '/test'", "cd -- '"+self.root.as_posix()+"'")
                        process.stdin.write((text+'\n').encode()); process.stdin.flush()
            except (EOFError,OSError): pass
            finally:
                if process.poll() is None: process.terminate()
                process.wait(timeout=5)
                channel.close()
        threading.Thread(target=run,daemon=True).start(); return True
    def check_channel_exec_request(self, channel, command):
        command = command.decode().replace("cd -- '/test'", "cd -- '" + self.root.as_posix() + "'")
        if os.name == 'nt': command = 'export PATH=/usr/bin:/bin; ' + command
        def run():
            process = subprocess.Popen([str(shell), '-c', command], stdout=subprocess.PIPE, stderr=subprocess.PIPE, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
            stdout, stderr = process.communicate(timeout=12)
            try: channel.sendall(stdout); channel.sendall_stderr(stderr); channel.send_exit_status(process.returncode)
            except (EOFError, OSError): pass
            finally: channel.close()
        threading.Thread(target=run, daemon=True).start(); return True

with tempfile.TemporaryDirectory(prefix='xpeed-real-ssh-') as temporary:
    temporary = Path(temporary).resolve(); private = paramiko.RSAKey.generate(2048)
    identity = temporary / 'id_rsa'; private.write_private_key_file(str(identity))
    if os.name == 'nt':
        script = "$fixtureUser=[System.Security.Principal.WindowsIdentity]::GetCurrent().User; $fixtureAcl=New-Object System.Security.AccessControl.FileSecurity; $fixtureAcl.SetOwner($fixtureUser); $fixtureAcl.SetAccessRuleProtection($true,$false); $fixtureAcl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($fixtureUser,'FullControl','Allow'))); Set-Acl -LiteralPath $env:XPEED_SSH_TEST_KEY -AclObject $fixtureAcl"
        subprocess.run([str(Path(os.environ['SystemRoot'])/'System32/WindowsPowerShell/v1.0/powershell.exe'), '-NoProfile', '-NonInteractive', '-Command', script], env=dict(os.environ, XPEED_SSH_TEST_KEY=str(identity)), check=True, capture_output=True)
    key = paramiko.RSAKey.generate(2048); hosts = temporary / 'known_hosts'; entries = []; listeners = []; transports = []
    stop = threading.Event()
    for name in ['SERVER-A', 'SERVER-B']:
        folder = temporary / name; folder.mkdir(); (folder / 'marker.txt').write_bytes((name+'\n').encode()); (folder / 'edit.txt').write_bytes(b'original\n')
        listener = socket.socket(); listener.bind(('127.0.0.1', 0)); listener.listen(); listener.settimeout(.2)
        port = listener.getsockname()[1]; listeners.append(listener)
        with hosts.open('a') as file: file.write(f'[127.0.0.1]:{port} {key.get_name()} {key.get_base64()}\n')
        entries.append({'connection': {'host':'127.0.0.1','port':port,'user':'xpeed-test','identity':str(identity)}, 'knownHosts':str(hosts)})
        def serve(listener=listener, folder=folder):
            while not stop.is_set():
                try: connection, _ = listener.accept()
                except socket.timeout: continue
                except OSError: return
                transport = paramiko.Transport(connection); transports.append(transport); transport.add_server_key(key)
                transport.set_subsystem_handler('sftp', paramiko.SFTPServer, Files)
                def start(transport=transport, folder=folder):
                    try:
                        transport.start_server(server=Server(folder, private))
                        while transport.is_active() and not stop.is_set(): time.sleep(.02)
                    except (EOFError, OSError, paramiko.SSHException): pass
                threading.Thread(target=start, daemon=True).start()
        threading.Thread(target=serve, daemon=True).start()
    config = temporary / 'fixtures.json'; config.write_text(json.dumps(entries))
    env = dict(os.environ, XPEED_TOOL_TEST_CONFIG=str(config), RUSTUP_HOME=str(root/'.tools/rustup'), CARGO_HOME=str(root/'.tools/cargo'))
    env['PATH'] = str(root/'.tools/cargo/bin')+os.pathsep+'C:/msys64/mingw64/bin'+os.pathsep+env['PATH']
    try:
        cargo = root/'.tools/cargo/bin/cargo.exe' if os.name == 'nt' else 'cargo'
        result = subprocess.run([str(cargo),'test','--locked','two_real_ssh_transports','--','--ignored','--nocapture'], cwd=root, env=env, timeout=180)
        if result.returncode: raise SystemExit(result.returncode)
        assert (temporary/'SERVER-A/edit.txt').read_text() == 'updated on A\n'
        assert (temporary/'SERVER-B/edit.txt').read_text() == 'original\n'
        print('PASS: real localhost SSH/SFTP, independent destinations, edits, timeouts and cancellation')
        result = subprocess.run([str(cargo),'test','--locked','visible_ssh_console','--','--ignored','--nocapture'], cwd=root, env=env, timeout=90)
        if result.returncode: raise SystemExit(result.returncode)
        print('PASS: visible SSH console commands preserve shell variables and return actual output')
        result = subprocess.run([str(cargo),'test','--locked','interactive_sftp_transfers_and_editor_conflicts','--','--ignored','--nocapture'], cwd=root, env=env, timeout=90)
        if result.returncode: raise SystemExit(result.returncode)
        print('PASS: binary upload/download, existing destination protection and editor conflict checks')
        if '--codex' in sys.argv:
            result = subprocess.run([str(cargo),'test','--locked','real_codex_executes_through_host_bridge','--','--ignored','--nocapture'], cwd=root, env=env, timeout=180)
            if result.returncode: raise SystemExit(result.returncode)
            print('PASS: real Codex executed ls through the XPEED bridge on an isolated SSH server')
    finally:
        stop.set()
        for listener in listeners: listener.close()
        for transport in transports: transport.close()
        time.sleep(3.5)

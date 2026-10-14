"""Executed with python3 on the selected server; no local file access."""
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import zipfile


def safe_name(name):
    path = PurePosixPath(name)
    if not name or path.is_absolute() or '..' in path.parts or '\\' in name or ':' in name or '\x00' in name:
        raise ValueError('Unsafe archive path: ' + repr(name))
    return path


def source_files(sources):
    for source in sources:
        root = Path(source)
        paths = [root]
        if root.is_dir() and not root.is_symlink():
            for parent, directories, files in os.walk(root, followlinks=False):
                paths.extend(Path(parent) / name for name in directories + files)
        for path in paths:
            mode = path.lstat().st_mode
            if not (stat.S_ISREG(mode) or stat.S_ISDIR(mode)):
                raise ValueError('Only regular files and directories can be archived: ' + str(path))
            yield path


def compress(sources, destination, kind):
    # Reject links/devices before creating the output or invoking an archiver.
    for _ in source_files(sources):
        pass
    with tempfile.TemporaryDirectory(prefix='.xpeed-archive-', dir='.') as temporary:
        output = Path(temporary) / destination.name
        if kind == 'rar':
            if not shutil.which('rar'):
                raise RuntimeError('Creating RAR requires the rar command on the server. Choose ZIP or TAR.GZ instead.')
            subprocess.run(['rar', 'a', '-r', '-idq', '-y', str(output), '--'] + sources, check=True)
        elif kind == 'zip':
            with zipfile.ZipFile(output, 'x', compression=zipfile.ZIP_DEFLATED, allowZip64=True) as archive:
                for path in source_files(sources):
                    print(str(path), flush=True)
                    archive.write(path, str(path))
        elif kind == 'tar.gz':
            with tarfile.open(output, 'x:gz') as archive:
                for path in source_files(sources):
                    print(str(path), flush=True)
                    archive.add(path, arcname=str(path), recursive=False)
        else:
            raise ValueError('Unsupported compression format')
        # Exclusive publication: never replace an existing archive.
        os.link(output, destination)


def extract(source, destination):
    lower = str(source).lower()
    if lower.endswith('.zip'):
        archive = zipfile.ZipFile(source)
        members = archive.infolist()
        def info(member):
            mode = member.external_attr >> 16
            if stat.S_ISLNK(mode) or (stat.S_IFMT(mode) not in (0, stat.S_IFREG, stat.S_IFDIR)):
                raise ValueError('Links and special entries are not supported')
            return member.filename, member.is_dir()
        opener = archive.open
    elif lower.endswith('.rar'):
        try:
            import rarfile
        except ImportError as error:
            raise RuntimeError('Extracting RAR requires Python rarfile and unrar/unar on the server.') from error
        archive = rarfile.RarFile(source)
        members = archive.infolist()
        def info(member):
            if not (member.isdir() or member.isfile()) or member.is_symlink() or member.file_redir:
                raise ValueError('Links and special entries are not supported')
            return member.filename, member.isdir()
        opener = archive.open
    else:
        archive = tarfile.open(source, 'r:*')
        members = archive.getmembers()
        def info(member):
            if not (member.isfile() or member.isdir()):
                raise ValueError('Links and special entries are not supported')
            return member.name, member.isdir()
        opener = archive.extractfile
    with archive:
        # Validate all members before writing. Extract manually into a new
        # private directory; archive permissions and links are never applied.
        validated = [(member, *info(member)) for member in members]
        for _, name, _ in validated:
            safe_name(name)
        destination.mkdir(mode=0o700)
        for member, name, directory in validated:
            target = destination.joinpath(*safe_name(name).parts)
            print(name, flush=True)
            if directory:
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with opener(member) as reader, target.open('xb') as writer:
                    shutil.copyfileobj(reader, writer, 1024 * 1024)


def main(request):
    os.chdir(request['cwd'])
    sources = request['sources']
    if not sources:
        raise ValueError('Select files first')
    for source in sources:
        safe_name(source)
    destination = Path(request['destination'])
    safe_name(str(destination))
    if len(destination.parts) != 1 or destination.exists() or destination.is_symlink():
        raise ValueError('Destination already exists or is not a simple name')
    if request['kind'] == 'extract':
        if len(sources) != 1:
            raise ValueError('Select one archive to extract')
        extract(sources[0], destination)
    else:
        compress(sources, destination, request['kind'])
    print('XPEED_ARCHIVE_OK', flush=True)


if __name__ == '__main__':
    try:
        main(json.loads(sys.argv[1]))
    except Exception as error:
        print(str(error), file=sys.stderr, flush=True)
        sys.exit(1)

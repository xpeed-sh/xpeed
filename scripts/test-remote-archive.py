"""Isolated archive tests: no SSH servers or personal files."""
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile

SCRIPT = Path(__file__).with_name('remote-archive.py')


class Archives(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        (self.root / 'folder').mkdir()
        (self.root / 'folder' / "hello ' world.txt").write_bytes(b'archive-content\x00\xff' * 10000)

    def tearDown(self):
        self.temporary.cleanup()

    def run_job(self, kind, sources, destination, success=True):
        result = subprocess.run([sys.executable, str(SCRIPT), json.dumps({
            'cwd': str(self.root), 'kind': kind, 'sources': sources, 'destination': destination
        })], capture_output=True, timeout=30)
        self.assertEqual(result.returncode == 0, success, result.stderr.decode(errors='replace'))
        self.assertEqual(b'XPEED_ARCHIVE_OK' in result.stdout, success)

    def test_zip_and_tar_roundtrip_and_collision(self):
        for kind in ('zip', 'tar.gz'):
            name = 'archive.' + kind
            self.run_job(kind, ['folder'], name)
            original = (self.root / name).read_bytes()
            self.run_job(kind, ['folder'], name, success=False)
            self.assertEqual((self.root / name).read_bytes(), original)
            self.run_job('extract', [name], kind + '-output')
            self.assertEqual((self.root / (kind + '-output') / 'folder' / "hello ' world.txt").read_bytes(),
                             (self.root / 'folder' / "hello ' world.txt").read_bytes())
            self.run_job('extract', [name], kind + '-output', success=False)

    def test_zip_traversal_and_links_are_rejected_before_destination_creation(self):
        for index, name in enumerate(('../escape', '/absolute', 'C:/absolute')):
            path = 'evil' + str(index) + '.zip'
            with zipfile.ZipFile(self.root / path, 'w') as archive:
                member = zipfile.ZipInfo('placeholder')
                member.filename = name
                archive.writestr(member, 'blocked')
            self.run_job('extract', [path], 'blocked', success=False)
            self.assertFalse((self.root / 'blocked').exists())
        import runpy
        safe_name = runpy.run_path(str(SCRIPT))['safe_name']
        with self.assertRaises(ValueError):
            safe_name('folder\\escape')
        with zipfile.ZipFile(self.root / 'link.zip', 'w') as archive:
            link = zipfile.ZipInfo('link')
            link.external_attr = 0o120777 << 16
            archive.writestr(link, '../outside')
        self.run_job('extract', ['link.zip'], 'blocked', success=False)
        self.assertFalse((self.root / 'blocked').exists())

    def test_tar_links_and_duplicate_members_are_rejected(self):
        with tarfile.open(self.root / 'link.tar', 'w') as archive:
            link = tarfile.TarInfo('link')
            link.type = tarfile.SYMTYPE
            link.linkname = '../outside'
            archive.addfile(link)
        self.run_job('extract', ['link.tar'], 'blocked', success=False)
        self.assertFalse((self.root / 'blocked').exists())
        with tarfile.open(self.root / 'duplicate.tar', 'w') as archive:
            for _ in range(2):
                member = tarfile.TarInfo('same')
                member.size = 3
                archive.addfile(member, io.BytesIO(b'one'))
        self.run_job('extract', ['duplicate.tar'], 'partial', success=False)
        self.assertEqual((self.root / 'partial' / 'same').read_bytes(), b'one')


if __name__ == '__main__':
    unittest.main()

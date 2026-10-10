"""A release archive contains the selected commit, not the server's worktree."""
import hashlib
import importlib.util
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("package_source", Path(__file__).with_name("package-source.py"))
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)


class PackagingTest(unittest.TestCase):
    def test_commit_only_and_secret_defense(self):
        with tempfile.TemporaryDirectory(prefix="dynodoc-package-test-") as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.check_output(["git", *args], cwd=root, stderr=subprocess.PIPE)
            git("init")
            git("config", "user.email", "test@example.test")
            git("config", "user.name", "Isolated packaging test")
            (root / "source.txt").write_text("committed source", encoding="utf-8")
            (root / ".env.example").write_text("PLACEHOLDER=", encoding="utf-8")
            (root / ".env").write_text("test fixture secret", encoding="utf-8")
            (root / "test.key").write_text("test fixture key", encoding="utf-8")
            git("add", ".")
            git("add", "--force", ".env", "test.key")
            git("commit", "-m", "fixture")
            commit = git("rev-parse", "HEAD").decode().strip()
            (root / "source.txt").write_text("uncommitted server change", encoding="utf-8")
            (root / "ops").mkdir()
            (root / "ops/status.json").write_text('{"private":"fixture"}', encoding="utf-8")
            (root / "staged.txt").write_text("staged but uncommitted", encoding="utf-8")
            git("add", "staged.txt")
            destination = root / "source.tar.gz"
            count, packaged = packager.package(root, destination)
            self.assertEqual(packaged, commit)
            self.assertEqual(count, 2)
            with tarfile.open(destination) as archive:
                self.assertEqual(set(archive.getnames()), {
                    "source.txt", ".env.example", "SOURCE_MANIFEST.sha256", "SOURCE_REVISION"
                })
                self.assertEqual(archive.extractfile("source.txt").read(), b"committed source")
                self.assertEqual(archive.extractfile("SOURCE_REVISION").read().decode().strip(), commit)
                manifest = archive.extractfile("SOURCE_MANIFEST.sha256").read().decode()
                self.assertIn(hashlib.sha256(b"committed source").hexdigest() + "  source.txt", manifest)

#!/usr/bin/env python3
"""Package a committed Git tree; never inspect untracked or modified worktree files."""
import hashlib
import io
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import tempfile

OMITTED = {
    ".git", ".codex", ".cursor", ".claude", ".agents", "node_modules", "target",
    ".svelte-kit", "build", ".vercel", "test-results", "playwright-report", "backups",
}


def allowed(name):
    path = PurePosixPath(name)
    return not (
        path.is_absolute() or ".." in path.parts
        or any(part in OMITTED for part in path.parts)
        or (path.name.startswith(".env") and path.name != ".env.example")
        or path.suffix in {".pem", ".key", ".dump"}
        or path.name.endswith(".tar.gz")
    )


def package(root, destination, revision="HEAD"):
    commit = subprocess.check_output(
        ["git", "rev-parse", "--verify", "--end-of-options", revision + "^{commit}"], cwd=root
    ).decode().strip()
    manifest = []
    with tempfile.TemporaryFile() as tree:
        subprocess.run(["git", "archive", "--format=tar", commit], cwd=root, stdout=tree, check=True)
        tree.seek(0)
        with tarfile.open(fileobj=tree, mode="r|") as source, tarfile.open(destination, "w:gz") as archive:
            for member in source:
                if not member.isfile() or not allowed(member.name):
                    continue
                content = source.extractfile(member).read()
                manifest.append(f"{hashlib.sha256(content).hexdigest()}  {member.name}\n")
                member.uid = member.gid = 0
                member.uname = member.gname = ""
                archive.addfile(member, io.BytesIO(content))
            for name, content in [
                ("SOURCE_REVISION", (commit + "\n").encode()),
                ("SOURCE_MANIFEST.sha256", "".join(manifest).encode()),
            ]:
                info = tarfile.TarInfo(name)
                info.size = len(content)
                info.mode = 0o644
                archive.addfile(info, io.BytesIO(content))
    return len(manifest), commit


if __name__ == "__main__":
    root = Path(__file__).resolve().parents[2]
    destination = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/dynodoc-source.tar.gz")
    count, commit = package(root, destination, sys.argv[2] if len(sys.argv) > 2 else "HEAD")
    print(f"Packaged {count} source files at {commit} into {destination}")

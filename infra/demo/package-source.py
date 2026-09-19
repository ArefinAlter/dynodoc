#!/usr/bin/env python3
"""Package the working source without environment files, dependencies or build output."""
import hashlib
import io
from pathlib import Path
import subprocess
import sys
import tarfile

root = Path(__file__).resolve().parents[2]
destination = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/dynodoc-source.tar.gz")
paths = subprocess.check_output(
    ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root
).decode().split("\0")
omitted = {
    ".git", ".codex", ".cursor", ".claude", ".agents", "node_modules", "target",
    ".svelte-kit", "build", ".vercel", "test-results", "playwright-report", "backups",
}
manifest = []
with tarfile.open(destination, "w:gz") as archive:
    for name in sorted(set(filter(None, paths))):
        path = root / name
        if any(part in omitted for part in path.relative_to(root).parts):
            continue
        if path.name.startswith(".env") and path.name != ".env.example":
            continue
        if path.suffix in {".pem", ".key", ".dump"} or path.name.endswith(".tar.gz"):
            continue
        if not path.is_file() or path.is_symlink() or path.resolve() == destination.resolve():
            continue
        content = path.read_bytes()
        manifest.append(f"{hashlib.sha256(content).hexdigest()}  {name}\n")
        info = archive.gettarinfo(str(path), arcname=name)
        info.uid = info.gid = 0
        info.uname = info.gname = ""
        archive.addfile(info, io.BytesIO(content))
    content = "".join(manifest).encode()
    info = tarfile.TarInfo("SOURCE_MANIFEST.sha256")
    info.size = len(content)
    info.mode = 0o644
    archive.addfile(info, io.BytesIO(content))
print(f"Packaged {len(manifest)} source files into {destination}")

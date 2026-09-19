"""Internal Word converter. No arbitrary paths or URLs are accepted."""
import http.server
import os
from pathlib import Path
import subprocess
import tempfile
import threading

slots = threading.BoundedSemaphore(2)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != '/convert' or self.headers.get('X-Converter-Key') != os.environ['CONVERTER_KEY']:
            self.send_error(403)
            return
        try:
            size = int(self.headers.get('Content-Length', '0'))
        except ValueError:
            self.send_error(400)
            return
        if not 0 < size <= 10 * 1024 * 1024:
            self.send_error(413)
            return
        if not slots.acquire(blocking=False):
            self.send_error(429, 'Conversion busy; try again shortly')
            return
        try:
            body = self.rfile.read(size)
            if not body.startswith(bytes.fromhex('d0cf11e0a1b11ae1')):
                self.send_error(400, 'Expected a binary Microsoft Word document')
                return
            with tempfile.TemporaryDirectory(prefix='dynodoc-') as folder:
                folder = Path(folder)
                source = folder / 'document.doc'
                source.write_bytes(body)
                profile = folder / 'profile'
                (profile / 'user').mkdir(parents=True)
                (profile / 'user' / 'registrymodifications.xcu').write_text('''<?xml version="1.0" encoding="UTF-8"?>
<oor:items xmlns:oor="http://openoffice.org/2001/registry"><item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="MacroSecurityLevel" oor:op="fuse"><value>3</value></prop></item><item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="DisableMacrosExecution" oor:op="fuse"><value>true</value></prop></item></oor:items>''')
                subprocess.run(['libreoffice', '-env:UserInstallation=' + profile.as_uri(), '--headless', '--norestore', '--nodefault', '--nolockcheck', '--convert-to', 'docx', '--outdir', str(folder), str(source)], timeout=45, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                output = folder / 'document.docx'
                if not output.exists() or output.stat().st_size > 20 * 1024 * 1024:
                    self.send_error(422, 'Could not convert this document')
                    return
                data = output.read_bytes()
                self.send_response(200)
                self.send_header('Content-Type', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document')
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                self.wfile.write(data)
        except (OSError, subprocess.SubprocessError):
            self.send_error(422, 'Conversion failed or timed out')
        finally:
            slots.release()
    def log_message(self, *_args):
        pass  # Never log document bodies or filenames.
http.server.ThreadingHTTPServer(('0.0.0.0', 8090), Handler).serve_forever()

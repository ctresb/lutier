#!/usr/bin/env python3
"""Servidor local do DAW do lutier.

Serve a UI estática de daw/ e expõe POST /render, que grava o par
.synth/.score num diretório temporário, roda o binário lutier e devolve
o WAV renderizado. Rodar a partir da raiz do repo:

    python3 daw/server.py [porta]
"""
import json
import os
import subprocess
import sys
import tempfile
from http.server import HTTPServer, SimpleHTTPRequestHandler

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(REPO, "target", "release", "lutier")
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8737


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=os.path.join(REPO, "daw"), **kwargs)

    def log_message(self, fmt, *args):
        sys.stderr.write("%s\n" % (fmt % args))

    def do_POST(self):
        if self.path != "/render":
            self.send_error(404)
            return
        try:
            n = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(n))
            synth_src = body["synth"]
            score_src = body["score"]
        except Exception:
            self.send_error(400, "bad request body")
            return

        with tempfile.TemporaryDirectory() as tmp:
            synth_path = os.path.join(tmp, "song.synth")
            score_path = os.path.join(tmp, "song.score")
            wav_path = os.path.join(tmp, "song.wav")
            with open(synth_path, "w") as f:
                f.write(synth_src)
            with open(score_path, "w") as f:
                f.write(score_src)
            # cwd = raiz do repo para os imports "presets/*.synth" resolverem
            proc = subprocess.run(
                [BIN, synth_path, score_path, "-o", wav_path],
                cwd=REPO, capture_output=True, text=True, timeout=120,
            )
            if proc.returncode != 0 or not os.path.exists(wav_path):
                msg = (proc.stderr or proc.stdout or "render falhou").strip()
                data = json.dumps({"error": msg}).encode()
                self.send_response(422)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
                return
            with open(wav_path, "rb") as f:
                wav = f.read()
        self.send_response(200)
        self.send_header("Content-Type", "audio/wav")
        self.send_header("Content-Length", str(len(wav)))
        self.end_headers()
        self.wfile.write(wav)


if __name__ == "__main__":
    if not os.path.exists(BIN):
        sys.exit("binário não encontrado: %s\nrode: cargo build --release" % BIN)
    print("lutier DAW em http://localhost:%d" % PORT)
    HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()

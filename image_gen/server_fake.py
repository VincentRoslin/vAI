"""Stdlib-only fake image sidecar — for CI and Rust `image::` unit tests.

Speaks the same HTTP contract as the real `server.py` (Phase 22.B) but loads
no model: `/generate` writes a solid-colour PNG per requested image into the
caller-provided `out_dir` and returns the paths + seeds. Bearer-authenticated
like the real one (ADR-0013 R-C1).

    python server_fake.py --port <p> --token <t> [--model-path ...] [--loras-dir ...]

No torch, no diffusers, no network. `--model-path` / `--quant-cache` /
`--loras-dir` are accepted and ignored so the launch argv matches the real
sidecar.
"""
import argparse
import json
import os
import struct
import threading
import time
import uuid
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PROTOCOL = 1

# Fixed tiny raster regardless of the requested size — nothing here decodes it,
# and the Rust side records the requested width/height from the request.
_FAKE_W = _FAKE_H = 32

_state = {"loaded": False}
_progress = {"active": False, "step": 0, "total_steps": 0, "image_index": 0, "batch_count": 0}
_lock = threading.Lock()


def _solid_png(rgb: tuple[int, int, int]) -> bytes:
    """A minimal valid PNG: one IHDR + one IDAT + IEND, solid colour."""
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", _FAKE_W, _FAKE_H, 8, 2, 0, 0, 0)  # 8-bit RGB
    row = b"\x00" + bytes(rgb) * _FAKE_W
    raw = row * _FAKE_H
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


class Handler(BaseHTTPRequestHandler):
    token = ""
    proto_ver = PROTOCOL

    def log_message(self, *_args):  # silence stderr access log
        pass

    def _send(self, code: int, body: dict):
        payload = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def _authed(self) -> bool:
        return self.headers.get("Authorization", "") == f"Bearer {self.token}"

    def do_GET(self):
        if self.path == "/health":
            self._send(
                200,
                {"status": "ok", "protocol": self.proto_ver, "loaded": _state["loaded"], "vram_mb": 0},
            )
            return
        if not self._authed():
            self._send(401, {"detail": "unauthorized"})
            return
        if self.path == "/progress":
            with _lock:
                self._send(200, dict(_progress))
            return
        self._send(404, {"detail": "not found"})

    def do_POST(self):
        if not self._authed():
            self._send(401, {"detail": "unauthorized"})
            return
        length = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(length) if length else b"{}"
        try:
            req = json.loads(raw or b"{}")
        except json.JSONDecodeError:
            self._send(400, {"detail": "bad json"})
            return

        if self.path == "/unload":
            _state["loaded"] = False
            self._send(200, {"status": "ok"})
            return

        if self.path == "/generate":
            self._generate(req)
            return

        self._send(404, {"detail": "not found"})

    def _generate(self, req: dict):
        out_dir = req.get("out_dir")
        if not out_dir:
            self._send(400, {"detail": "out_dir is required"})
            return
        batch = max(1, int(req.get("batch_count", 1)))
        total_steps = max(1, int(req.get("steps", 8)))
        base_seed = int(req.get("seed", int(time.time())))
        os.makedirs(out_dir, exist_ok=True)
        _state["loaded"] = True

        images = []
        for i in range(batch):
            for step in range(total_steps):
                with _lock:
                    _progress.update(
                        active=True, step=step + 1, total_steps=total_steps,
                        image_index=i, batch_count=batch,
                    )
                time.sleep(0.01)
            colour = ((base_seed + i) % 256, (i * 40) % 256, 128)
            path = os.path.join(out_dir, f"{uuid.uuid4().hex}.png")
            with open(path, "wb") as f:
                f.write(_solid_png(colour))
            images.append({"path": path, "seed": base_seed + i})

        with _lock:
            _progress["active"] = False
        self._send(200, {"images": images})


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--token", type=str, required=True)
    parser.add_argument("--model-path", type=str, default=None)
    parser.add_argument("--quant-cache", type=str, default=None)
    parser.add_argument("--loras-dir", type=str, default=None)
    parser.add_argument("--protocol", type=int, default=PROTOCOL, help="override for the mismatch test")
    args = parser.parse_args()

    Handler.token = args.token
    Handler.proto_ver = args.protocol
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.serve_forever()


if __name__ == "__main__":
    main()

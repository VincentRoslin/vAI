#!/usr/bin/env python3
"""Fake TTS worker — stdlib only, no venv, no GPU.

Speaks the ADR-0013 protocol so `voice::` tests can exercise the playback +
barge-in path without Chatterbox. Writes a short mono 16 kHz sine WAV whose
length is proportional to the text. Modes via the payload: "ok" | "crash" |
"slow".
"""

import json
import math
import struct
import sys
import time
import wave

PROTOCOL_VERSION = 1
SR = 24000


def emit(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def write_sine_wav(path, seconds):
    import os

    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    n = max(1, int(seconds * SR))
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        frames = bytearray()
        for i in range(n):
            v = int(0.2 * 32767 * math.sin(2 * math.pi * 180 * i / SR))
            frames += struct.pack("<h", v)
        w.writeframes(bytes(frames))
    return n / SR


def main():
    emit({"protocol_version": PROTOCOL_VERSION, "worker": "Tts"})

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        req = json.loads(line)
        job_id = req["id"]
        payload = req.get("payload") or {}
        mode = payload.get("mode", "ok")
        text = payload.get("text", "")
        out_path = payload.get("out_path")

        if payload.get("warm"):
            emit(
                {
                    "id": job_id,
                    "result": {
                        "status": "Ok",
                        "body": {"data": {"sample_rate": SR, "duration_s": 0.0}},
                    },
                }
            )
            continue

        if mode == "crash":
            sys.stderr.write("fake tts: simulated crash\n")
            sys.stderr.flush()
            sys.exit(1)
        if mode == "slow":
            time.sleep(1.5)

        try:
            seconds = max(0.3, len(text) * 0.05)
            dur = write_sine_wav(out_path, seconds)
            emit(
                {
                    "id": job_id,
                    "result": {
                        "status": "Ok",
                        "body": {"data": {"sample_rate": SR, "duration_s": dur}},
                    },
                }
            )
        except Exception as e:  # noqa: BLE001
            emit(
                {
                    "id": job_id,
                    "result": {
                        "status": "Err",
                        "body": {"error": {"kind": "BackendUnavailable", "message": f"fake tts: {e}"}},
                    },
                }
            )


if __name__ == "__main__":
    main()

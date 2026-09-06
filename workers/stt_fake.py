#!/usr/bin/env python3
"""Fake STT worker — stdlib only, no venv, no GPU.

Speaks the ADR-0013 protocol so `worker::` and `voice::` unit tests can exercise
the supervisor and the voice pipeline without faster-whisper. Behaviour is driven
by the request payload:

    {"audio_path": "...", "mode": "ok"|"empty"|"lowconf"|"crash"|"progress",
     "text": "override transcript"}

Default mode "ok" returns a canned transcript.
"""

import json
import sys

PROTOCOL_VERSION = 1
CANNED = "hello local AI this is a fake transcript"


def emit(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def main():
    emit({"protocol_version": PROTOCOL_VERSION, "worker": "Stt"})

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        req = json.loads(line)
        job_id = req["id"]
        payload = req.get("payload") or {}
        mode = payload.get("mode", "ok")

        if mode == "crash":
            sys.stderr.write("fake worker: simulated crash\n")
            sys.stderr.flush()
            sys.exit(1)

        if mode == "progress":
            for p in (0.25, 0.5, 0.75):
                emit(
                    {
                        "id": job_id,
                        "result": {
                            "status": "Progress",
                            "body": {"progress": p, "detail": None},
                        },
                    }
                )

        if mode == "empty":
            text = ""
            no_speech = 0.9
        elif mode == "lowconf":
            text = "mmhmm"
            no_speech = 0.1
        else:
            text = payload.get("text", CANNED)
            no_speech = 0.02

        emit(
            {
                "id": job_id,
                "result": {
                    "status": "Ok",
                    "body": {
                        "data": {
                            "text": text,
                            "language": "en",
                            "duration_s": 1.5,
                            "avg_logprob": -3.0 if mode == "lowconf" else -0.3,
                            "no_speech_prob": no_speech,
                        }
                    },
                },
            }
        )


if __name__ == "__main__":
    main()

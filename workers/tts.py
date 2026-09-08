#!/usr/bin/env python3
"""TTS worker (Phase 19) — Resemble Chatterbox Turbo (English), CUDA.

Protocol: ADR-0013 stdio JSON-lines. First line is a WorkerHello; then one
WorkerResponse per WorkerRequest.

Request  payload: {"text": "one clause of assistant text.",
                   "out_path": "<abs WAV path the Rust core dictates>",
                   "voice_wav": "<abs reference-WAV path, or null for the builtin>",
                   "exaggeration": 0.65, "cfg_weight": 0.3}
              or  {"warm": true, "voice_wav": "<abs path or null>"}
                   to load / `prepare_conditionals` without synthesising.
Response Ok.data: {"sample_rate": 24000, "duration_s": 1.8}

`voice_wav` clones a speaker from a reference clip (Chatterbox
`prepare_conditionals`). It is an absolute path the Rust core dictates — the
worker never chooses one. The prepared conditionals are cached, so switching
voice costs one extra ~1-2 s prep on the first clause only.

The model loads from a local directory only (ADR-0015, env LOCALAI_TTS_MODEL_DIR
= <models.dir>/tts). Output is a mono 16-bit PCM WAV the Rust core reads, plays,
and deletes.
"""

import json
import os
import sys

PROTOCOL_VERSION = 1

# Chatterbox + resemble-perth + s3tokenizer print status ("loaded PerthNet…",
# "S3 Token -> Mel Inference…") to stdout, which would corrupt the JSON-lines
# protocol. Keep a private handle to the real stdout for `emit`, and point
# `sys.stdout` at stderr for everything else.
_PROTO = sys.stdout
sys.stdout = sys.stderr


def _assert_offline_env():
    required = {
        "HF_HUB_OFFLINE": "1",
        "TRANSFORMERS_OFFLINE": "1",
        "HF_HUB_DISABLE_TELEMETRY": "1",
    }
    missing = [k for k, v in required.items() if os.environ.get(k) != v]
    if missing:
        raise RuntimeError(f"offline env not set by the supervisor: {missing}")


def emit(obj):
    _PROTO.write(json.dumps(obj) + "\n")
    _PROTO.flush()


def log(msg):
    sys.stderr.write(f"{msg}\n")
    sys.stderr.flush()


def main():
    _assert_offline_env()

    model_dir = os.environ.get("LOCALAI_TTS_MODEL_DIR")
    if not model_dir or not os.path.isdir(model_dir):
        raise RuntimeError(f"LOCALAI_TTS_MODEL_DIR missing or not a dir: {model_dir!r}")

    import time

    import numpy as np
    import soundfile as sf
    from chatterbox.tts_turbo import ChatterboxTurboTTS

    t0 = time.monotonic()
    model = ChatterboxTurboTTS.from_local(model_dir, device="cuda")
    sr = int(model.sr)
    log(f"tts: model loaded in {time.monotonic() - t0:.2f}s from {model_dir} (sr={sr})")

    # `from_local` loaded the built-in voice (conds.pt) into `model.conds`. Keep
    # a handle so we can switch back after speaking in a cloned voice.
    builtin_conds = model.conds
    current_voice = None  # None = built-in; otherwise the reference-WAV path

    def select_voice(voice_wav):
        nonlocal current_voice
        if voice_wav == current_voice:
            return
        if voice_wav is None:
            model.conds = builtin_conds
        else:
            t = time.monotonic()
            model.prepare_conditionals(voice_wav)
            log(f"tts: prepared cloned voice from {voice_wav} in {time.monotonic() - t:.2f}s")
        current_voice = voice_wav

    emit({"protocol_version": PROTOCOL_VERSION, "worker": "Tts"})

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            job_id = req["id"]
        except (ValueError, KeyError) as e:
            log(f"tts: bad request line: {e}")
            continue

        payload = req.get("payload") or {}
        text = (payload.get("text") or "").strip()
        out_path = payload.get("out_path")
        voice_wav = payload.get("voice_wav") or None
        exaggeration = float(payload.get("exaggeration") or 0.65)
        cfg_weight = float(payload.get("cfg_weight") or 0.3)

        try:
            if payload.get("warm"):
                select_voice(voice_wav)
                emit(
                    {
                        "id": job_id,
                        "result": {
                            "status": "Ok",
                            "body": {"data": {"sample_rate": sr, "duration_s": 0.0}},
                        },
                    }
                )
                continue
            if not text:
                raise ValueError("empty text")
            if not out_path:
                raise ValueError("no out_path")
            if voice_wav and not os.path.isfile(voice_wav):
                raise ValueError(f"voice_wav not found: {voice_wav}")
            select_voice(voice_wav)
            t0 = time.monotonic()
            try:
                wav = model.generate(text, exaggeration=exaggeration, cfg_weight=cfg_weight)
            except TypeError:
                wav = model.generate(text)
            audio = np.asarray(wav.squeeze().detach().cpu().numpy(), dtype=np.float32)
            os.makedirs(os.path.dirname(out_path), exist_ok=True)
            sf.write(out_path, audio, sr, subtype="PCM_16")
            dur = len(audio) / sr
            emit(
                {
                    "id": job_id,
                    "result": {
                        "status": "Ok",
                        "body": {"data": {"sample_rate": sr, "duration_s": dur}},
                    },
                }
            )
            log(f"tts: {dur:.2f}s audio in {time.monotonic() - t0:.2f}s (rtf {(time.monotonic()-t0)/max(dur,1e-6):.2f})")
        except Exception as e:  # noqa: BLE001 — report any failure as a typed Err
            kind = "Validation" if isinstance(e, (ValueError,)) else "BackendUnavailable"
            emit(
                {
                    "id": job_id,
                    "result": {
                        "status": "Err",
                        "body": {"error": {"kind": kind, "message": f"tts: {e}"}},
                    },
                }
            )


if __name__ == "__main__":
    main()

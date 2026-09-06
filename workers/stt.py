#!/usr/bin/env python3
"""STT worker (Phase 18) — faster-whisper large-v3, fp16, CUDA.

Protocol: ADR-0013 stdio JSON-lines. First line is a WorkerHello; then one
WorkerResponse per WorkerRequest read from stdin.

Request  payload: {"audio_path": "<abs 16 kHz mono WAV>", "language": null|"en"}
Response Ok.data: {"text", "language", "duration_s", "avg_logprob",
                   "no_speech_prob"}

The model is loaded from a local directory only (ADR-0015 — never a repo id).
CTranslate2 needs the CUDA 12 / cuDNN 9 user-space DLLs; on Windows we add the
venv's `nvidia/*/bin` dirs to the loader path before importing ctranslate2.
"""

import glob
import json
import os
import sys

PROTOCOL_VERSION = 1


def _prime_cuda_dll_path():
    if os.name != "nt":
        return
    base = os.path.join(sys.prefix, "Lib", "site-packages", "nvidia")
    dirs = glob.glob(os.path.join(base, "*", "bin"))
    if dirs:
        os.environ["PATH"] = os.pathsep.join(dirs) + os.pathsep + os.environ.get("PATH", "")
        for d in dirs:
            try:
                os.add_dll_directory(d)
            except OSError:
                pass


def _assert_offline_env():
    """ADR-0015 — fail loud if the supervisor did not lock the network down."""
    required = {
        "HF_HUB_OFFLINE": "1",
        "TRANSFORMERS_OFFLINE": "1",
        "HF_HUB_DISABLE_TELEMETRY": "1",
    }
    missing = [k for k, v in required.items() if os.environ.get(k) != v]
    if missing:
        raise RuntimeError(f"offline env not set by the supervisor: {missing}")


def emit(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def log(msg):
    sys.stderr.write(f"{msg}\n")
    sys.stderr.flush()


def main():
    _assert_offline_env()
    _prime_cuda_dll_path()

    model_dir = os.environ.get("LOCALAI_STT_MODEL_DIR")
    if not model_dir or not os.path.isdir(model_dir):
        raise RuntimeError(f"LOCALAI_STT_MODEL_DIR missing or not a dir: {model_dir!r}")

    from faster_whisper import WhisperModel

    t0 = _now()
    model = WhisperModel(model_dir, device="cuda", compute_type="float16")
    log(f"stt: model loaded in {_now() - t0:.2f}s from {model_dir}")

    emit({"protocol_version": PROTOCOL_VERSION, "worker": "Stt"})

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            job_id = req["id"]
        except (ValueError, KeyError) as e:
            log(f"stt: bad request line: {e}")
            continue

        payload = req.get("payload") or {}
        audio_path = payload.get("audio_path")
        language = payload.get("language")

        try:
            if not audio_path or not os.path.isfile(audio_path):
                raise FileNotFoundError(f"audio_path not found: {audio_path!r}")
            t0 = _now()
            segments, info = model.transcribe(audio_path, language=language, vad_filter=False)
            seg_list = list(segments)
            text = "".join(s.text for s in seg_list).strip()
            avg_logprob = (
                sum(s.avg_logprob for s in seg_list) / len(seg_list) if seg_list else 0.0
            )
            no_speech = (
                sum(s.no_speech_prob for s in seg_list) / len(seg_list) if seg_list else 1.0
            )
            emit(
                {
                    "id": job_id,
                    "result": {
                        "status": "Ok",
                        "body": {
                            "data": {
                                "text": text,
                                "language": info.language,
                                "duration_s": info.duration,
                                "avg_logprob": avg_logprob,
                                "no_speech_prob": no_speech,
                            }
                        },
                    },
                }
            )
            log(f"stt: '{text[:40]}' rtf={( _now() - t0) / max(info.duration, 1e-6):.3f}")
        except Exception as e:  # noqa: BLE001 — report any failure as a typed Err
            kind = "Validation" if isinstance(e, (FileNotFoundError, ValueError)) else "BackendUnavailable"
            emit(
                {
                    "id": job_id,
                    "result": {
                        "status": "Err",
                        "body": {"error": {"kind": kind, "message": f"stt: {e}"}},
                    },
                }
            )


def _now():
    import time

    return time.monotonic()


if __name__ == "__main__":
    main()

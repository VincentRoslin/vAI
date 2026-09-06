"""One-time NF4 quantization of Krea 2 Turbo's transformer + text encoder.

Run by the Rust acquisition step (`FixedModel::Image`) via the dev venv, once,
when `<models.dir>/image/quant_cache/krea2/` is absent or its fingerprint no
longer matches the installed library versions. The sidecar (`server.py`) only
ever *reads* this cache — it never quantizes (ADR-0006).

    python quantize.py --model-path <id-or-dir> --out <quant_cache/krea2>

Writes `<out>/transformer/`, `<out>/text_encoder/`, `<out>/fingerprint.json`.
Progress goes to stdout as `PROGRESS <phase>` lines the Rust step surfaces:
`checking-ram`, `loading-transformer`, `loading-text-encoder`, `saving`, `done`.

Memory: the bf16 transformer is ~24 GB resident during this step. Gated on
`free_ram > 26 GB` up front — on a machine that can't meet that, copy a cache
built elsewhere instead (see docs/plan/22 §Phase-entry).
"""
import argparse
import hashlib
import json
import os
import sys

# Must stay identical to server.py's copy — a cache written here is validated
# there by an exact dict compare.
_QUANT_RECIPE_VERSION = 1
_QUANT_KWARGS = {
    "load_in_4bit": True,
    "bnb_4bit_quant_type": "nf4",
    "bnb_4bit_compute_dtype": "torch.bfloat16",
}
_MIN_FREE_RAM_BYTES = 26 * 1024**3


def _progress(phase: str):
    print(f"PROGRESS {phase}", flush=True)


def _free_ram_bytes() -> int:
    try:
        import psutil
        return int(psutil.virtual_memory().available)
    except Exception:
        pass
    # psutil is not a hard dep; fall back to the OS call on Windows.
    if os.name == "nt":
        import ctypes

        class _MS(ctypes.Structure):
            _fields_ = [
                ("dwLength", ctypes.c_ulong), ("dwMemoryLoad", ctypes.c_ulong),
                ("ullTotalPhys", ctypes.c_ulonglong), ("ullAvailPhys", ctypes.c_ulonglong),
                ("ullTotalPageFile", ctypes.c_ulonglong), ("ullAvailPageFile", ctypes.c_ulonglong),
                ("ullTotalVirtual", ctypes.c_ulonglong), ("ullAvailVirtual", ctypes.c_ulonglong),
                ("ullAvailExtendedVirtual", ctypes.c_ulonglong),
            ]

        ms = _MS()
        ms.dwLength = ctypes.sizeof(_MS)
        ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(ms))
        return int(ms.ullAvailPhys)
    try:
        return os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_AVPHYS_PAGES")
    except (ValueError, OSError, AttributeError):
        return _MIN_FREE_RAM_BYTES  # can't tell — don't block


def _fingerprint(model_id: str) -> dict:
    import bitsandbytes
    import diffusers
    import torch
    import transformers

    return {
        "recipe_version": _QUANT_RECIPE_VERSION,
        "model_id": model_id,
        "diffusers": diffusers.__version__,
        "transformers": transformers.__version__,
        "bitsandbytes": bitsandbytes.__version__,
        "torch": torch.__version__,
        "quant_kwargs_hash": hashlib.sha256(
            repr(sorted(_QUANT_KWARGS.items())).encode()
        ).hexdigest(),
    }


def _write_fingerprint(out_dir: str, model_id: str):
    fp = os.path.join(out_dir, "fingerprint.json")
    tmp = fp + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(_fingerprint(model_id), f)
    os.replace(tmp, fp)   # crash-safe: never a half-written fingerprint


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--model-path", required=True, help="repo id or diffusers dir (bf16)")
    ap.add_argument("--model-id", default=None,
                    help="id recorded in fingerprint.json (default: --model-path)")
    ap.add_argument("--out", required=True, help="quant-cache dir to (over)write")
    ap.add_argument("--skip-ram-check", action="store_true", help="testing only")
    args = ap.parse_args()

    _progress("checking-ram")
    if not args.skip_ram_check:
        free = _free_ram_bytes()
        if free < _MIN_FREE_RAM_BYTES:
            print(
                f"ERROR: {free / 1e9:.1f} GB free RAM, need ~{_MIN_FREE_RAM_BYTES / 1e9:.0f} GB "
                "to quantize the bf16 transformer. Free memory or copy a cache built elsewhere.",
                file=sys.stderr,
            )
            return 2

    import torch
    from diffusers import AutoModel as DiffusersAutoModel
    from diffusers.quantizers.quantization_config import BitsAndBytesConfig as DBnB
    from transformers import AutoModel as TransformersAutoModel
    from transformers import BitsAndBytesConfig as TBnB

    kwargs = dict(load_in_4bit=True, bnb_4bit_quant_type="nf4",
                  bnb_4bit_compute_dtype=torch.bfloat16)

    _progress("loading-transformer")
    transformer = DiffusersAutoModel.from_pretrained(
        args.model_path, subfolder="transformer",
        quantization_config=DBnB(**kwargs), dtype=torch.bfloat16, low_cpu_mem_usage=True,
    )
    _progress("loading-text-encoder")
    text_encoder = TransformersAutoModel.from_pretrained(
        args.model_path, subfolder="text_encoder",
        quantization_config=TBnB(**kwargs), dtype=torch.bfloat16, low_cpu_mem_usage=True,
    )

    _progress("saving")
    os.makedirs(args.out, exist_ok=True)
    transformer.save_pretrained(os.path.join(args.out, "transformer"))
    text_encoder.save_pretrained(os.path.join(args.out, "text_encoder"))
    _write_fingerprint(args.out, args.model_id or args.model_path)
    _progress("done")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

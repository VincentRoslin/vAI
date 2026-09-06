"""LocalAI image-generation sidecar — Krea 2 Turbo, text-to-image only.

One process, one resident model. Adapted *down* from a working sibling project
(see docs/plan/22 — reference to adapt, not copy). What this keeps from there:
the fingerprinted NF4 quant-cache load, the LoRA state-dict converter (3
in-the-wild formats, drop-don't-raise on unmappable layers), the single-adapter
apply + registration check, `_snap_wh`, and the step-progress callback.

What is deliberately gone: Z-Image, `krea2_edit` / img2img / `/edit` (Krea 2 is
text-to-image only for v1), the sidecar's own VRAM 503 gate (Rust owns VRAM —
ADR-0006), the directory-scan LoRA list (Rust holds the registry and passes a
bare filename), the safety/quality negative auto-append (the caller's `negative`
is passed through verbatim; it is inert at guidance 0 anyway), and lazy
re-quantize on load — the quant cache is built once at acquisition
(`quantize.py`) and this process only ever *reads* it.

Wire contract (see src-tauri/src/image/protocol.rs — PROTOCOL_VERSION):

  GET  /health    -> {status, protocol, loaded, vram_mb}      (no auth)
  GET  /progress  -> {active, step, total_steps, image_index, batch_count}
  POST /generate  -> {images: [{path, seed}, ...]}   writes PNGs into out_dir
  POST /unload    -> {status: "ok"}

All routes except /health require `Authorization: Bearer <--token>` (ADR-0013
R-C1 — a bare loopback server is callable by any local process).

Binary exchange (Article I): images are never returned inline. Each PNG is
written into the `out_dir` the Rust core dictates in the request body; the
core reads, blob-stores, and deletes them.

Bound to 127.0.0.1 only.
"""
import argparse
import gc
import hashlib
import io
import json
import os
import threading
import time
import uuid
from typing import Optional

# --- protocol version: bump on any breaking change to a shape below; must
# match src-tauri/src/image/protocol.rs::PROTOCOL_VERSION. Overridable via
# --protocol for the Rust mismatch test.
PROTOCOL_VERSION = 1

# The ungated diffusers-format mirror of Krea 2 Turbo (the official
# krea/Krea-2-Turbo is gated). Acquisition ensures this is in the HF cache;
# this process loads it offline. Override with KREA2_MODEL_ID.
DEFAULT_MODEL_ID = os.environ.get("KREA2_MODEL_ID", "unsloth/Krea-2-Turbo")

# Bump if the quantization recipe changes in a way that should invalidate an
# existing cache even though no library version moved. Kept identical to the
# reference + quantize.py so a cache written by either validates here.
_QUANT_RECIPE_VERSION = 1


# ------------------------------------------------------------------ CLI / state

def _parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--port", type=int, required=True)
    p.add_argument("--token", type=str, required=True)
    # Path (diffusers layout) or a repo id resolvable from the offline HF cache.
    p.add_argument("--model-path", type=str, default=DEFAULT_MODEL_ID)
    # The id recorded in the quant cache's fingerprint (provenance). Kept
    # separate from --model-path so Rust can pass a real on-disk snapshot dir
    # for the offline load while still matching a cache fingerprinted against
    # the repo id "unsloth/Krea-2-Turbo".
    p.add_argument("--model-id", type=str, default=DEFAULT_MODEL_ID)
    p.add_argument("--quant-cache", type=str, required=True,
                   help="NF4 quant-cache dir with transformer/ text_encoder/ fingerprint.json")
    p.add_argument("--loras-dir", type=str, default=None)
    p.add_argument("--protocol", type=int, default=PROTOCOL_VERSION,
                   help="override the reported protocol (Rust mismatch test)")
    return p.parse_args()


ARGS = _parse_args()

_model_lock = threading.Lock()   # serialises load / generate / unload
_progress_lock = threading.Lock()
_progress = {"active": False, "step": 0, "total_steps": 0, "image_index": 0, "batch_count": 0}

pipe = None
_loaded = False
# (filename, weight) currently applied to `pipe`, or None.
current_lora: Optional[tuple[str, float]] = None


# ------------------------------------------------------------------ quant cache

_QUANT_KWARGS = {
    "load_in_4bit": True,
    "bnb_4bit_quant_type": "nf4",
    "bnb_4bit_compute_dtype": "torch.bfloat16",   # stringified for the fingerprint
}


def _quant_fingerprint(model_id: str) -> dict:
    import bitsandbytes
    import diffusers
    import transformers
    import torch  # noqa: F401 (kept for parity with the writer)

    return {
        "recipe_version": _QUANT_RECIPE_VERSION,
        "model_id": model_id,
        "diffusers": diffusers.__version__,
        "transformers": transformers.__version__,
        "bitsandbytes": bitsandbytes.__version__,
        "torch": __import__("torch").__version__,
        "quant_kwargs_hash": hashlib.sha256(
            repr(sorted(_QUANT_KWARGS.items())).encode()
        ).hexdigest(),
    }


def _cache_state(model_id: str) -> str:
    """'ok' | 'absent' | 'stale'. A stale/corrupt cache is never loaded
    silently — that would mean weights quantized under a different recipe or
    library version than the one running now (subtly-wrong-but-image-shaped
    output). The sidecar does not re-quantize; acquisition owns that."""
    d = ARGS.quant_cache
    fp = os.path.join(d, "fingerprint.json")
    if not (os.path.isdir(os.path.join(d, "transformer"))
            and os.path.isdir(os.path.join(d, "text_encoder"))
            and os.path.isfile(fp)):
        return "absent"
    try:
        with open(fp, "r", encoding="utf-8") as f:
            stored = json.load(f)
    except (OSError, json.JSONDecodeError):
        return "stale"
    return "ok" if stored == _quant_fingerprint(model_id) else "stale"


def _load_quantized_components(model_id: str):
    """(transformer, text_encoder) read from the NF4 quant cache. Raises
    RuntimeError if the cache is absent or fingerprint-stale — the handler
    turns that into a 503 telling the user to run image-model setup."""
    import torch
    from diffusers import AutoModel as DiffusersAutoModel
    from transformers import AutoModel as TransformersAutoModel

    state = _cache_state(model_id)
    if state != "ok":
        raise RuntimeError(
            f"NF4 quant cache is {state} at {ARGS.quant_cache} — run image-model "
            "setup (acquisition builds it once with quantize.py). The sidecar "
            "never quantizes."
        )
    transformer = DiffusersAutoModel.from_pretrained(
        os.path.join(ARGS.quant_cache, "transformer"), dtype=torch.bfloat16
    )
    text_encoder = TransformersAutoModel.from_pretrained(
        os.path.join(ARGS.quant_cache, "text_encoder"), dtype=torch.bfloat16
    )
    return transformer, text_encoder


# ------------------------------------------------------------------ model load

def _set_progress(**kw):
    with _progress_lock:
        _progress.update(**kw)


def _vram_mb() -> int:
    try:
        import torch
        if torch.cuda.is_available():
            return int(torch.cuda.memory_allocated() / (1024 * 1024))
    except Exception:
        pass
    return 0


def _unload_locked():
    global pipe, _loaded, current_lora
    if pipe is not None:
        del pipe
        pipe = None
    _loaded = False
    current_lora = None
    try:
        import torch
        gc.collect()
        torch.cuda.empty_cache()
    except Exception:
        pass


def _ensure_loaded_locked():
    """Load Krea 2 if it is not resident. Caller holds _model_lock."""
    global pipe, _loaded
    if _loaded and pipe is not None:
        return
    import torch
    from diffusers import Krea2Pipeline

    transformer, text_encoder = _load_quantized_components(ARGS.model_id)
    pipe = Krea2Pipeline.from_pretrained(
        ARGS.model_path,
        dtype=torch.bfloat16,
        transformer=transformer,
        text_encoder=text_encoder,
        low_cpu_mem_usage=True,
    )
    # CPU offload so a 12.9B model fits a 16 GB card. Rust has already evicted
    # the LLM before calling us (ADR-0010, manual until Phase 23).
    pipe.enable_model_cpu_offload()
    _loaded = True


# ------------------------------------------------------------------ LoRA

# Top-level submodules of Krea2Transformer2DModel a LoRA can target — used to
# spot a state dict already in diffusers naming but missing the "transformer."
# prefix.
_KREA2_DIFFUSERS_ROOTS = (
    "transformer_blocks", "text_fusion", "img_in", "txt_in", "final_layer",
    "time_embed", "time_mod_proj", "x_embedder", "context_embedder",
)


def _convert_lora_to_diffusers(raw: dict) -> dict:
    """Best-effort conversion of a Krea 2 LoRA state dict to what
    Krea2Pipeline.load_lora_weights expects (`transformer.<module>.lora_A/B`).
    Handles already-diffusers, prefix-less diffusers, and Krea-native /
    ComfyUI / Ostris AI-Toolkit naming in PEFT or Kohya form. Never raises on
    an unmappable key — drops and counts it; the caller checks something
    registered."""
    import re

    alphas = {mod[:-6]: float(v) for mod, v in
              ((k, raw[k]) for k in raw if k.endswith(".alpha"))}
    sd = {}
    for k, v in raw.items():
        if k.endswith(".alpha"):
            continue
        nk = k.replace(".lora_down.weight", ".lora_A.weight").replace(
            ".lora_up.weight", ".lora_B.weight")
        sd[nk] = v
    for mod, alpha in alphas.items():
        a_key, b_key = f"{mod}.lora_A.weight", f"{mod}.lora_B.weight"
        if b_key in sd and a_key in sd:
            rank = sd[a_key].shape[0]
            if rank:
                sd[b_key] = sd[b_key] * (alpha / rank)

    if all(k.startswith("transformer.") or k.startswith("text_encoder.") for k in sd):
        return sd

    stripped = {k.removeprefix("base_model.model.").removeprefix("diffusion_model."): v
                for k, v in sd.items()}

    if any(k.startswith(_KREA2_DIFFUSERS_ROOTS) for k in stripped):
        return {
            f"transformer.{k}": v
            for k, v in stripped.items()
            if k.startswith(_KREA2_DIFFUSERS_ROOTS) and re.search(r"\.lora_[AB]\.weight$", k)
        }

    attn_map = {"wq": "to_q", "wk": "to_k", "wv": "to_v", "wo": "to_out.0", "gate": "to_gate"}
    ff_map = {"gate": "ff.gate", "up": "ff.up", "down": "ff.down"}
    standalone_map = {
        "first": "img_in", "last.linear": "final_layer.linear",
        "tmlp.0": "time_embed.linear_1", "tmlp.2": "time_embed.linear_2",
        "tproj.1": "time_mod_proj", "txtmlp.1": "txt_in.linear_1",
        "txtmlp.3": "txt_in.linear_2", "txtfusion.projector": "text_fusion.projector",
    }

    def convert_module(module: str):
        m = re.match(r"blocks\.(\d+)\.(attn|mlp)\.(\w+)$", module)
        if m:
            idx, kind, sub = m.groups()
            if kind == "attn" and sub in attn_map:
                return f"transformer_blocks.{idx}.attn.{attn_map[sub]}"
            if kind == "mlp" and sub in ff_map:
                return f"transformer_blocks.{idx}.{ff_map[sub]}"
            return None
        m = re.match(r"txtfusion\.(layerwise_blocks|refiner_blocks)\.(\d+)\.(attn|mlp)\.(\w+)$", module)
        if m:
            block, idx, kind, sub = m.groups()
            if kind == "attn" and sub in attn_map:
                return f"text_fusion.{block}.{idx}.attn.{attn_map[sub]}"
            if kind == "mlp" and sub in ff_map:
                return f"text_fusion.{block}.{idx}.{ff_map[sub]}"
            return None
        return standalone_map.get(module)

    out, dropped = {}, 0
    for key, val in stripped.items():
        match = re.search(r"\.lora_[AB]\.weight$", key)
        if match is None:
            continue
        mod = convert_module(key[: match.start()])
        if mod is None:
            dropped += 1
            continue
        out[f"transformer.{mod}{key[match.start():]}"] = val
    if dropped:
        print(f"[lora] mapped {len(out) // 2} modules, skipped {dropped // 2} unsupported",
              flush=True)
    return out


def _lora_path(name: str) -> str:
    """Resolve a bare filename to a path inside --loras-dir, rejecting anything
    that climbs out. Rust already validated against the registry; this is the
    defence-in-depth re-check (ADR-0006)."""
    if not ARGS.loras_dir:
        raise ValueError("no --loras-dir configured")
    if os.path.basename(name) != name or not name.lower().endswith(".safetensors"):
        raise ValueError(f"invalid LoRA name: {name}")
    path = os.path.join(ARGS.loras_dir, name)
    if not os.path.isfile(path):
        raise ValueError(f"LoRA not found: {name}")
    return path


def _apply_lora_locked(name: Optional[str], weight: float):
    """Bring the resident pipe to exactly (name, weight). No-op when it already
    matches. Caller holds _model_lock."""
    global current_lora
    from safetensors.torch import load_file

    target = (name, float(weight)) if name else None
    if target == current_lora:
        return
    if current_lora is not None:
        pipe.unload_lora_weights()
        current_lora = None
    if target is None:
        return

    converted = _convert_lora_to_diffusers(load_file(_lora_path(name)))
    if not converted:
        raise ValueError(
            f"'{name}' is not in a LoRA format this build can load (Krea reference "
            "trainer / Ostris AI-Toolkit, PEFT or Kohya naming work; LyCORIS/LoKr do not)."
        )
    pipe.load_lora_weights(converted, adapter_name="realism")
    if "realism" not in pipe.get_list_adapters().get("transformer", []):
        pipe.unload_lora_weights()
        raise ValueError(f"'{name}' loaded no usable layers.")
    pipe.set_adapters(["realism"], adapter_weights=[float(weight)])
    current_lora = target


# ------------------------------------------------------------------ generation

def _snap_wh(width: int, height: int) -> tuple[int, int]:
    def snap(n: int) -> int:
        return max(16, int(round(n / 16) * 16))
    return snap(width), snap(height)


def _make_progress_callback(image_index: int, batch_count: int, total_steps: int):
    def _cb(_pipe, step: int, _t, kw: dict) -> dict:
        _set_progress(active=True, step=step + 1, total_steps=total_steps,
                      image_index=image_index, batch_count=batch_count)
        return kw
    return _cb


# Krea 2 Turbo is distilled for an 8-step, guidance-0 schedule.
_DEFAULT_STEPS = 8
_DEFAULT_GUIDANCE = 0.0


def _generate_locked(body: dict) -> list[dict]:
    """Run the batch. Caller holds _model_lock. Returns [{path, seed}, ...]."""
    import torch

    out_dir = body.get("out_dir")
    if not out_dir:
        raise ValueError("out_dir is required")
    prompt = body.get("prompt") or ""
    negative = body.get("negative")
    steps = int(body.get("steps") or _DEFAULT_STEPS)
    guidance = body.get("guidance")
    guidance = _DEFAULT_GUIDANCE if guidance is None else float(guidance)
    width, height = _snap_wh(int(body.get("width") or 1024), int(body.get("height") or 1024))
    batch = max(1, int(body.get("batch_count") or 1))
    seed = body.get("seed")
    seed = int(time.time() * 1000) % (2 ** 31) if seed is None else int(seed)

    lora = body.get("lora")
    if lora:
        _apply_lora_locked(lora.get("file"), float(lora.get("weight", 0.9)))
    else:
        _apply_lora_locked(None, 0.0)

    os.makedirs(out_dir, exist_ok=True)
    results: list[dict] = []
    try:
        for i in range(batch):
            this_seed = seed + i
            gen = torch.Generator(device="cuda").manual_seed(this_seed)
            try:
                image = pipe(
                    prompt=prompt,
                    negative_prompt=negative,   # inert at guidance 0
                    height=height,
                    width=width,
                    num_inference_steps=steps,
                    guidance_scale=guidance,
                    generator=gen,
                    callback_on_step_end=_make_progress_callback(i, batch, steps),
                ).images[0]
            except torch.cuda.OutOfMemoryError as e:
                raise MemoryError(f"CUDA OOM during generation: {e}") from e

            path = os.path.join(out_dir, f"{uuid.uuid4().hex}.png")
            buf = io.BytesIO()
            image.save(buf, format="PNG")
            with open(path, "wb") as f:
                f.write(buf.getvalue())
            results.append({"path": os.path.abspath(path), "seed": this_seed})
    finally:
        _set_progress(active=False)
    return results


# ------------------------------------------------------------------ HTTP app

from fastapi import Depends, FastAPI, Header, HTTPException  # noqa: E402
from fastapi.responses import JSONResponse  # noqa: E402
from pydantic import BaseModel  # noqa: E402

app = FastAPI()


def _require_token(authorization: str = Header(default="")):
    if authorization != f"Bearer {ARGS.token}":
        raise HTTPException(status_code=401, detail="unauthorized")


class LoraBody(BaseModel):
    file: str
    weight: float = 0.9


class GenerateBody(BaseModel):
    prompt: str
    negative: Optional[str] = None
    width: int = 1024
    height: int = 1024
    steps: int = _DEFAULT_STEPS
    guidance: float = _DEFAULT_GUIDANCE
    seed: Optional[int] = None
    batch_count: int = 1
    out_dir: str
    lora: Optional[LoraBody] = None


@app.exception_handler(Exception)
async def _unhandled(_req, exc: Exception):
    # Anything not already an HTTPException becomes a typed body the Rust
    # client's map_sidecar_error understands (503 for OOM, 400 for bad input,
    # 500 otherwise) — never a bare stack trace / hang.
    if isinstance(exc, HTTPException):
        return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail})
    if isinstance(exc, MemoryError):
        return JSONResponse(status_code=503, content={"detail": str(exc)})
    if isinstance(exc, (ValueError, KeyError)):
        return JSONResponse(status_code=400, content={"detail": str(exc)})
    if isinstance(exc, RuntimeError):
        return JSONResponse(status_code=503, content={"detail": str(exc)})
    return JSONResponse(status_code=500, content={"detail": f"{type(exc).__name__}: {exc}"})


@app.get("/health")
def health():
    return {"status": "ok", "protocol": ARGS.protocol, "loaded": _loaded, "vram_mb": _vram_mb()}


@app.get("/progress", dependencies=[Depends(_require_token)])
def progress():
    with _progress_lock:
        return dict(_progress)


@app.post("/unload", dependencies=[Depends(_require_token)])
def unload():
    with _model_lock:
        _unload_locked()
    _set_progress(active=False)
    return {"status": "ok"}


@app.post("/generate", dependencies=[Depends(_require_token)])
def generate(body: GenerateBody):
    payload = body.model_dump()
    # Validate the LoRA name before the (expensive) load, so a bad name fails
    # fast rather than after a 20 s model load.
    if body.lora is not None:
        _lora_path(body.lora.file)
    with _model_lock:
        _ensure_loaded_locked()
        images = _generate_locked(payload)
    return {"images": images}


def main():
    import uvicorn
    uvicorn.run(app, host="127.0.0.1", port=ARGS.port, log_level="warning")


if __name__ == "__main__":
    main()

# Phase 11 — Model Registry

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Model metadata as data: stable id, display name, type (LLM/STT/TTS/image),
backend, file path, capabilities, context size, quantization, estimated resource
requirement, supported devices, model-specific config. No model loading.

## Depends on
Phase 9 (persistence), Phase 7 (contracts).

## Not in this phase
- Loading, running, or resource-reserving a model.
- Downloading a model (Phase 12).
- A generic "any backend" abstraction beyond what LLM + the fixed STT/TTS/image
  models need.

## Architecture notes
- Registry entries are rows; the registry is queried, not hardcoded.
- Missing files and unsupported models are *represented states*, not errors that
  crash.
- `ModelId` is stable across restarts and path changes.
- Capabilities are queryable (e.g. "supports streaming", "context ≥ N").

## Performance notes
- Registry reads are frequent (every model selection) — indexed lookups, cache in
  memory with invalidation on write.

## Step outline
1. Schema (migration): model table + capability representation.
2. Repository: insert, get-by-id, list, list-by-type, update, delete.
3. Path validation: a registered path must exist and be inside the configured
   model dir; a missing file → `state: missing`, not a failure.
4. Metadata ingestion helper (used by Phase 12): from a GGUF header / a fixed
   model manifest → a registry entry.
5. Capability query API.
6. Invalid-metadata rejection (bad type, negative context, path traversal).
7. In-memory cache with write invalidation.
8. Tests: register, find, read metadata, detect missing path, query capability,
   reject invalid, stable id across a simulated restart.

## Verification gate
1. Register a test model; find it; read its metadata back correctly.
2. A registered model whose file is removed shows `state: missing`, no crash.
3. Capability query returns correct results.
4. Invalid metadata (each failure mode) is rejected.
5. A path outside the model dir is refused.
6. `ModelId` is stable across a restart.

## ADRs / open questions
- Capability schema shape (enum set vs free-form flags).

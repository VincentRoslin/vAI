-- Phase 11: the model registry. Metadata as data — one row per acquired model
-- (LLM / STT / TTS / image / embedder). No loading, no downloads (Phase 12),
-- no LoRAs / presets (Phase 22).

CREATE TABLE model_entry (
    id                TEXT PRIMARY KEY NOT NULL,   -- UUIDv4 (ADR-0017)
    display_name      TEXT NOT NULL,
    kind              TEXT NOT NULL,               -- Llm | Stt | Tts | Image | Embedder
    backend           TEXT NOT NULL,               -- opaque runtime id
    quant             TEXT,                        -- NULL = unquantized
    path              TEXT NOT NULL,               -- absolute, confined to the model dir
    streaming         INTEGER NOT NULL,            -- 0 | 1
    context_tokens    INTEGER,                     -- NULL when N/A
    estimated_vram_mb INTEGER,                     -- NULL when unknown
    devices           TEXT NOT NULL,               -- JSON array, e.g. ["Cuda","Cpu"]
    config            TEXT NOT NULL,               -- JSON object, per-backend knobs
    created_at        TEXT NOT NULL,               -- RFC-3339
    updated_at        TEXT NOT NULL
) STRICT;

CREATE INDEX model_entry_kind_idx ON model_entry (kind);

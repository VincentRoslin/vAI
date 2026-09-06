-- Phase 22: Image generation + the content-addressed blob store.
--
-- The blob store (`asset`) is general infrastructure that finally lands here as
-- the first feature to store a binary blob (ADR-0009; deferred since Phase 9).
-- Voice turns become `Audio { asset, transcript }` on top of it later,
-- additively. Bytes live on disk at `app_data/blobs/<sha[0:2]>/<sha>`; this
-- table is the metadata + the referential anchor. Write order (ADR-0009): the
-- blob file is fsynced before the row that references it commits.
--
-- Image generation (ADR-0006): Krea 2 Turbo is the only model. LoRA selection
-- and prompts are UNTRUSTED — a LoRA id resolves through `image_lora` to a
-- basename-validated file inside the confined loras dir; the prompt is opaque
-- text handed to the diffusers sidecar.

-- Content-addressed blob. id = lowercase hex SHA-256 of the bytes.
CREATE TABLE asset (
    id          TEXT PRIMARY KEY NOT NULL,   -- AssetId = sha256 hex
    media_type  TEXT NOT NULL,               -- e.g. "image/png"
    byte_len    INTEGER NOT NULL,
    created_at  TEXT NOT NULL                -- RFC-3339
) STRICT;

-- Realism LoRAs available to Krea 2 (ADR-0006 LoRA registry). `file` is a bare
-- basename inside the confined loras dir; the row is only seeded when the file
-- is actually present.
CREATE TABLE image_lora (
    id             TEXT PRIMARY KEY NOT NULL,  -- ImageLoraId (UUIDv4, ADR-0017)
    file           TEXT NOT NULL UNIQUE,       -- basename, *.safetensors
    display_name   TEXT NOT NULL,
    base_compat    TEXT NOT NULL,              -- "krea2"
    format         TEXT NOT NULL,              -- "peft" | "kohya" | "diffusers" | "unknown"
    default_weight REAL NOT NULL,              -- 0..1
    tags           TEXT NOT NULL DEFAULT '[]', -- JSON array of strings
    created_at     TEXT NOT NULL
) STRICT;

-- Named parameter sets (ADR-0006 preset registry).
CREATE TABLE image_preset (
    id          TEXT PRIMARY KEY NOT NULL,     -- ImagePresetId (UUIDv4)
    name        TEXT NOT NULL UNIQUE,
    params      TEXT NOT NULL,                 -- JSON: {width,height,steps,guidance}
    created_at  TEXT NOT NULL
) STRICT;

-- One generated image: what it is and how it was made. `asset_id` is a hard FK
-- (the blob must exist first); `lora_id` is a soft link (a LoRA row may be
-- removed later — a dangling id just means the history UI shows no LoRA name).
CREATE TABLE generated_image (
    id           TEXT PRIMARY KEY NOT NULL,    -- GeneratedImageId (UUIDv4)
    asset_id     TEXT NOT NULL REFERENCES asset(id),
    prompt       TEXT NOT NULL,
    negative     TEXT,
    width        INTEGER NOT NULL,
    height       INTEGER NOT NULL,
    steps        INTEGER NOT NULL,
    guidance     REAL NOT NULL,
    seed         INTEGER NOT NULL,
    lora_id      TEXT,                          -- NULL = base model (no FK — soft link)
    lora_weight  REAL,
    model_id     TEXT NOT NULL,                 -- registry ModelId of Krea 2
    created_at   TEXT NOT NULL
) STRICT;

CREATE INDEX generated_image_created_idx ON generated_image (created_at DESC);

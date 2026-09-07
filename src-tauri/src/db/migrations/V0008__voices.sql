-- Phase 19 follow-up (course insert): cloned TTS voices. Chatterbox Turbo
-- clones a speaker from a short reference WAV; this table is the registry of
-- imported references. The WAV bytes live on disk at
-- `<models.dir>/tts/voices/<file>` (Rust owns that path — CLAUDE.md Article I);
-- this row is the metadata + the "which one is active" flag.
--
-- `name` is a user label (untrusted text — only ever shown in the UI, never fed
-- to a model). At most one row has `active = 1`; NULL/none active = the model's
-- built-in `conds.pt` voice.

CREATE TABLE voice (
    id         TEXT PRIMARY KEY NOT NULL,   -- VoiceId (UUIDv4, ADR-0017)
    name       TEXT NOT NULL,               -- user label, non-blank, trimmed
    file       TEXT NOT NULL,               -- basename inside <models.dir>/tts/voices/
    active     INTEGER NOT NULL DEFAULT 0,  -- 0/1
    created_at TEXT NOT NULL                -- RFC-3339
) STRICT;

-- Enforce "at most one active voice" at the schema level.
CREATE UNIQUE INDEX voice_one_active ON voice (active) WHERE active = 1;

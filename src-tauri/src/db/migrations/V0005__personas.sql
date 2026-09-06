-- Phase 20: Personas & Context Builder. A Persona is structured behaviour data
-- for a Tab 1 conversation (FR-30..35): a small fixed set of text fields the
-- context builder renders into the system block. Persona text is UNTRUSTED —
-- the builder sanitises it before it enters the prompt (SECURITY C2).

CREATE TABLE persona (
    id          TEXT PRIMARY KEY NOT NULL,      -- PersonaId (UUIDv4, ADR-0017)
    name        TEXT NOT NULL,
    summary     TEXT NOT NULL DEFAULT '',       -- one-line "who they are"
    personality TEXT NOT NULL DEFAULT '',
    tone        TEXT NOT NULL DEFAULT '',
    style       TEXT NOT NULL DEFAULT '',       -- communication style
    guidance    TEXT NOT NULL DEFAULT '[]',     -- JSON array of do/don't lines
    created_at  TEXT NOT NULL,                  -- RFC-3339
    updated_at  TEXT NOT NULL
) STRICT;

-- A conversation may be bound to one Persona; NULL = the default assistant.
-- Fixed once the conversation has a turn (enforced in ConversationRepo).
ALTER TABLE conversation
    ADD COLUMN persona_id TEXT REFERENCES persona(id) ON DELETE SET NULL;

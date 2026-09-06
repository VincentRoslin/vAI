-- Phase 21: Memory. Persistent per-Persona (later per-Character) memory —
-- durable facts extracted from conversations, retrieved by BM25 keyword search
-- and fed into the one context builder's memory slot. Memory content is
-- UNTRUSTED (it originated from model output / user text) — the builder
-- sanitises it before it enters a prompt (SECURITY C2, ADR-0012).

CREATE TABLE memory (
    id          TEXT PRIMARY KEY NOT NULL,      -- MemoryId (UUIDv4, ADR-0017)
    scope       TEXT NOT NULL,                  -- "persona:<id>" (later "character:<id>")
    kind        TEXT NOT NULL,                  -- Fact | Preference | Event | Trait
    content     TEXT NOT NULL,                  -- the memory text (sanitised at store)
    importance  INTEGER NOT NULL,               -- 1..5
    -- Provenance. Best-effort, no FKs: a memory outlives the conversation /
    -- message it came from (both may be pruned); a dangling id just means the
    -- "view memories" UI can't link back.
    source_conversation_id TEXT,
    source_message_id      TEXT,
    created_at  TEXT NOT NULL                   -- RFC-3339
) STRICT;

CREATE INDEX memory_scope_idx ON memory (scope);

-- Standalone FTS5 (not external-content): the repo writes `memory` and
-- `memory_fts` in the same transaction, so no sync triggers are needed.
-- `mem_id` / `scope` are unindexed payload columns; `content` is searchable.
CREATE VIRTUAL TABLE memory_fts USING fts5 (
    mem_id UNINDEXED,
    scope  UNINDEXED,
    content,
    tokenize = 'unicode61 remove_diacritics 2'
);

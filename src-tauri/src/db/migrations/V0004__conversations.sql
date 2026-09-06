-- Phase 16: the first vertical slice — text chat. Minimal conversation +
-- message storage. One shape for Persona and Character conversations
-- (`kind`); voice / image message content and richer metadata come in later
-- phases (17, 18-22).

CREATE TABLE conversation (
    id          TEXT PRIMARY KEY NOT NULL,   -- ConversationId (UUIDv4, ADR-0017)
    kind        TEXT NOT NULL,               -- Persona | Character
    title       TEXT,                        -- NULL until named
    created_at  TEXT NOT NULL,               -- RFC-3339
    updated_at  TEXT NOT NULL                -- bumped on every appended message
) STRICT;

CREATE TABLE message (
    id               TEXT PRIMARY KEY NOT NULL,   -- MessageId (UUIDv4)
    conversation_id  TEXT NOT NULL
                        REFERENCES conversation(id) ON DELETE CASCADE,
    role             TEXT NOT NULL,               -- System | User | Assistant
    content          TEXT NOT NULL,               -- JSON: contracts MessageContent
    created_at       TEXT NOT NULL,               -- RFC-3339
    -- generation provenance (assistant messages only; NULL otherwise)
    gen_model        TEXT,
    gen_stop_reason  TEXT,                        -- EndOfText|MaxTokens|StopSequence|Cancelled|Error
    gen_tokens       INTEGER,
    gen_duration_ms  INTEGER
) STRICT;

-- Messages are ordered by insertion (implicit rowid) within a conversation.
CREATE INDEX message_conversation_idx ON message (conversation_id);

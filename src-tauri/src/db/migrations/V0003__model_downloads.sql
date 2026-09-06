-- Phase 12: download queue / progress state. One row per file transfer, so the
-- picker and resume-on-restart survive a hard kill. Completed rows are kept for
-- history until the model is deleted.

CREATE TABLE model_downloads (
    id                TEXT PRIMARY KEY NOT NULL,   -- DownloadId (UUIDv4)
    repo              TEXT NOT NULL,               -- HF repo, e.g. "owner/name"
    revision          TEXT NOT NULL,               -- git ref, e.g. "main"
    filename          TEXT NOT NULL,               -- path within the repo
    kind              TEXT NOT NULL,               -- Llm | Stt | Tts | Embedder
    dest_path         TEXT NOT NULL,               -- absolute, confined to the model dir
    total_bytes       INTEGER,                     -- NULL until known
    downloaded_bytes  INTEGER NOT NULL,            -- resume offset
    sha256_expected   TEXT,                        -- HF LFS hash, NULL if unknown
    etag              TEXT,
    state             TEXT NOT NULL,               -- Queued|Downloading|Paused|Verifying|Complete|Failed
    error             TEXT,                        -- last failure message
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
) STRICT;

CREATE INDEX model_downloads_state_idx ON model_downloads (state);

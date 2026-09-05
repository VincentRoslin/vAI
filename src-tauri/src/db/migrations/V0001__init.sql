-- Phase 9 minimal schema: a key/value table under the `core_` group, enough to
-- exercise CRUD, transactions, and migration. Feature tables (`conv_`, `char_`,
-- `mem_`, `model_`, `asset_`) arrive with their phases.

CREATE TABLE core_app_meta (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

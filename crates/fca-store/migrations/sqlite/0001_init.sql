-- Initial schema. Amounts are signed minor units (cents): < 0 expense, > 0 income.
-- Dates are ISO-8601 text (YYYY-MM-DD), which sorts correctly as plain text.

CREATE TABLE recurring (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    description    TEXT    NOT NULL,
    amount         INTEGER NOT NULL,
    currency       TEXT    NOT NULL,
    account        TEXT    NOT NULL,
    payee          TEXT,
    schedule       TEXT    NOT NULL,           -- canonical form, e.g. '2w', '1m'
    start_date     TEXT    NOT NULL,
    end_date       TEXT,
    last_generated TEXT,
    active         INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE transactions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    date         TEXT    NOT NULL,
    amount       INTEGER NOT NULL,
    currency     TEXT    NOT NULL,
    description  TEXT    NOT NULL,
    account      TEXT    NOT NULL,             -- where the money moved
    payee        TEXT,                         -- the other party
    origin       TEXT    NOT NULL CHECK (origin IN ('manual', 'import', 'recurring')),
    recurring_id INTEGER REFERENCES recurring (id) ON DELETE SET NULL,
    import_hash  TEXT    UNIQUE,               -- makes re-importing a file a no-op
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE (recurring_id, date)                -- one generated row per recurring entry and day
);
CREATE INDEX transactions_date ON transactions (date);

CREATE TABLE labels (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE transaction_labels (
    transaction_id INTEGER NOT NULL REFERENCES transactions (id) ON DELETE CASCADE,
    label_id       INTEGER NOT NULL REFERENCES labels (id) ON DELETE CASCADE,
    PRIMARY KEY (transaction_id, label_id)
);

CREATE TABLE recurring_labels (
    recurring_id INTEGER NOT NULL REFERENCES recurring (id) ON DELETE CASCADE,
    label_id     INTEGER NOT NULL REFERENCES labels (id) ON DELETE CASCADE,
    PRIMARY KEY (recurring_id, label_id)
);

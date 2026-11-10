-- Account settings (opening balances) and transfers between accounts.

-- Optional record per account. Transactions keep the plain account name, so an
-- account without a row simply has opening balance 0.
CREATE TABLE accounts (
    name            TEXT    PRIMARY KEY COLLATE NOCASE,
    currency        TEXT    NOT NULL,
    opening_balance INTEGER NOT NULL DEFAULT 0   -- minor units, before the first transaction
);

-- The two sides of a transfer share the same transfer_id (the id of the outgoing side).
ALTER TABLE transactions ADD COLUMN transfer_id INTEGER;
CREATE INDEX transactions_transfer ON transactions (transfer_id) WHERE transfer_id IS NOT NULL;

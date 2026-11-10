-- A recurring entry with transfer_to set generates a transfer from `account` to
-- `transfer_to` on every occurrence, instead of a single income or expense.
ALTER TABLE recurring ADD COLUMN transfer_to TEXT;

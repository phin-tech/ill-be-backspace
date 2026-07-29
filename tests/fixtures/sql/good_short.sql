-- Partial index: only live rows are ever queried.
CREATE INDEX idx ON t (a) WHERE deleted_at IS NULL;

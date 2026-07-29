-- This migration backfills the tenant column. It does NOT rewrite history,
-- because the audit table is append-only and rewriting it would invalidate
-- the signature chain. Verified 2026-07-29: running this twice is safe, the
-- second run is a no-op thanks to the WHERE clause. If you need to re-run it
-- after a schema change, drop the index first or the backfill takes hours.
UPDATE t SET tenant = 1 WHERE tenant IS NULL;

-- A preview is an immutable normalized plan, never a mutation of the master data.
-- Customers have no stable customer code yet; create-only imports use a trimmed,
-- case-insensitive name to avoid silently duplicating an existing customer.
CREATE TABLE master_import_batches (
    id uuid PRIMARY KEY,
    kind text NOT NULL CHECK (kind IN ('customers','suppliers','products')),
    filename text NOT NULL CHECK (length(filename) BETWEEN 1 AND 120),
    delimiter text NOT NULL CHECK (delimiter IN ('comma','semicolon')),
    plan_hash text NOT NULL CHECK (plan_hash ~ '^[0-9a-f]{64}$'),
    rows jsonb NOT NULL CHECK (jsonb_typeof(rows)='array'),
    errors jsonb NOT NULL CHECK (jsonb_typeof(errors)='array'),
    row_count integer NOT NULL CHECK (row_count BETWEEN 0 AND 1000),
    valid_count integer NOT NULL CHECK (valid_count BETWEEN 0 AND row_count),
    status text NOT NULL CHECK (status IN ('ready','invalid','committed')),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    committed_at timestamptz,
    created_count integer NOT NULL DEFAULT 0,
    commit_key uuid UNIQUE,
    CHECK (jsonb_array_length(rows)=row_count),
    CHECK ((status='invalid' AND jsonb_array_length(errors)>0 AND created_count=0 AND committed_at IS NULL AND commit_key IS NULL)
        OR (status='ready' AND jsonb_array_length(errors)=0 AND row_count>0 AND valid_count=row_count AND created_count=0 AND committed_at IS NULL AND commit_key IS NULL)
        OR (status='committed' AND jsonb_array_length(errors)=0 AND row_count>0 AND valid_count=row_count AND created_count=row_count AND committed_at IS NOT NULL AND commit_key IS NOT NULL))
);
CREATE INDEX master_import_batches_recent ON master_import_batches(created_at DESC,id DESC);

CREATE FUNCTION guard_master_import_batch() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.status <> 'ready' OR NEW.status <> 'committed'
        OR (to_jsonb(NEW)-ARRAY['status','committed_at','created_count','commit_key'])
            IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','committed_at','created_count','commit_key']) THEN
        RAISE EXCEPTION 'An import preview is immutable and can only be committed once';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER master_import_batch_transition BEFORE UPDATE ON master_import_batches
    FOR EACH ROW EXECUTE FUNCTION guard_master_import_batch();
CREATE TRIGGER master_import_batch_delete BEFORE DELETE ON master_import_batches
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER master_import_batch_truncate BEFORE TRUNCATE ON master_import_batches
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();

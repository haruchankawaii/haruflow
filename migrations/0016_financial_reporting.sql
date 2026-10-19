-- Reporting annotations never alter a posted amount or accounting date.
CREATE TABLE cash_flow_classifications (
    journal_id uuid PRIMARY KEY REFERENCES journal_entries(id),
    category text CHECK (category IN ('operating','investing','financing')),
    reason text NOT NULL CHECK (length(reason) BETWEEN 3 AND 1000),
    version bigint NOT NULL CHECK (version > 0),
    updated_by uuid NOT NULL REFERENCES users(id),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE cash_flow_classification_events (
    id uuid PRIMARY KEY,
    journal_id uuid NOT NULL REFERENCES cash_flow_classifications(journal_id),
    category text CHECK (category IN ('operating','investing','financing')),
    previous_category text CHECK (previous_category IN ('operating','investing','financing')),
    reason text NOT NULL CHECK (length(reason) BETWEEN 3 AND 1000),
    version bigint NOT NULL CHECK (version > 0),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (journal_id,version)
);
CREATE FUNCTION guard_cash_flow_classification() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.journal_id <> OLD.journal_id OR NEW.version <> OLD.version+1 THEN
        RAISE EXCEPTION 'Reporting classification identity and version are immutable';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER cash_flow_classification_version BEFORE UPDATE ON cash_flow_classifications
    FOR EACH ROW EXECUTE FUNCTION guard_cash_flow_classification();
CREATE TRIGGER immutable_cash_flow_classification_delete BEFORE DELETE ON cash_flow_classifications
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_cash_flow_classification_truncate BEFORE TRUNCATE ON cash_flow_classifications
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_cash_flow_classification_events BEFORE UPDATE OR DELETE ON cash_flow_classification_events
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_cash_flow_classification_events_truncate BEFORE TRUNCATE ON cash_flow_classification_events
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();

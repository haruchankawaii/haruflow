ALTER TABLE journal_entries DROP CONSTRAINT journal_entries_source_type_check;
ALTER TABLE journal_entries ADD CONSTRAINT journal_entries_source_type_check
    CHECK (source_type IN ('receipt','shipment','invoice','payment','manual','adjustment','reversal','opening'));
ALTER TABLE posting_reversals DROP CONSTRAINT posting_reversals_source_type_check;
ALTER TABLE posting_reversals ADD CONSTRAINT posting_reversals_source_type_check
    CHECK (source_type IN ('payment','invoice','shipment','opening'));

CREATE TABLE opening_documents (
    id uuid PRIMARY KEY,
    company_id uuid NOT NULL UNIQUE REFERENCES company(id),
    status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','posted')),
    version integer NOT NULL CHECK (version > 0),
    description text NOT NULL CHECK (length(description) BETWEEN 1 AND 500),
    items jsonb NOT NULL CHECK (jsonb_typeof(items)='array' AND jsonb_array_length(items)<=100),
    -- Up to 100 manually entered lines plus the generated inventory debit.
    lines jsonb NOT NULL CHECK (jsonb_typeof(lines)='array' AND jsonb_array_length(lines)<=101),
    active_posting_id uuid UNIQUE,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((status='draft' AND active_posting_id IS NULL) OR
           (status='posted' AND active_posting_id IS NOT NULL))
);

-- A posting is an immutable snapshot. Reversal is recorded separately, so all
-- earlier descriptions, product names and amounts survive later draft edits.
CREATE TABLE opening_postings (
    id uuid PRIMARY KEY,
    document_id uuid NOT NULL REFERENCES opening_documents(id),
    document_version integer NOT NULL CHECK (document_version > 0),
    journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),
    description text NOT NULL,
    items jsonb NOT NULL CHECK (jsonb_typeof(items)='array'),
    lines jsonb NOT NULL CHECK (jsonb_typeof(lines)='array'),
    inventory_total numeric(18,2) NOT NULL CHECK (inventory_total >= 0),
    total_debit numeric(18,2) NOT NULL CHECK (total_debit > 0),
    total_credit numeric(18,2) NOT NULL CHECK (total_credit=total_debit),
    created_by uuid NOT NULL REFERENCES users(id),
    posted_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (document_id,document_version),
    UNIQUE (id,document_id)
);
ALTER TABLE opening_documents ADD CONSTRAINT opening_active_document_fk
    FOREIGN KEY (active_posting_id,id) REFERENCES opening_postings(id,document_id);
CREATE TRIGGER immutable_opening_postings BEFORE UPDATE OR DELETE ON opening_postings
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_opening_postings_truncate BEFORE TRUNCATE ON opening_postings
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();

ALTER TABLE inventory_movements ADD COLUMN opening_posting_id uuid REFERENCES opening_postings(id);
ALTER TABLE inventory_movements DROP CONSTRAINT inventory_movements_kind_check;
ALTER TABLE inventory_movements DROP CONSTRAINT inventory_movements_direction_check;
ALTER TABLE inventory_movements ADD CONSTRAINT inventory_movements_kind_check
    CHECK (kind IN ('receipt','shipment','adjustment','shipment_reversal','opening','opening_reversal'));
ALTER TABLE inventory_movements ADD CONSTRAINT inventory_movements_direction_check CHECK (
    (kind IN ('receipt','shipment_reversal','opening') AND quantity>0) OR
    (kind IN ('shipment','opening_reversal') AND quantity<0) OR
    (kind='adjustment' AND quantity<>0));
ALTER TABLE inventory_movements ADD CONSTRAINT inventory_opening_reference_check CHECK (
    (kind IN ('opening','opening_reversal') AND opening_posting_id IS NOT NULL) OR
    (kind NOT IN ('opening','opening_reversal') AND opening_posting_id IS NULL));
CREATE UNIQUE INDEX inventory_opening_product_idx ON inventory_movements(opening_posting_id,product_id)
    WHERE kind='opening';

-- The bytes and their audit trail travel with the same PostgreSQL backup as
-- the document. No public file path or external storage credential is needed.
CREATE TABLE document_attachments (
    id uuid PRIMARY KEY,
    document_type text NOT NULL CHECK (document_type IN ('sales_order','invoice','payment','purchase_order','purchase_receipt','purchase_bill','supplier_payment','journal','sales_return','supplier_return')),
    document_id uuid NOT NULL,
    filename text NOT NULL CHECK (char_length(filename) BETWEEN 1 AND 200 AND filename !~ '[[:cntrl:]/\\]' AND filename NOT IN ('.','..') AND filename=btrim(filename)),
    media_type text NOT NULL CHECK (media_type IN ('application/pdf','image/png','image/jpeg')),
    content bytea NOT NULL CHECK (octet_length(content) BETWEEN 1 AND 5242880),
    byte_size integer GENERATED ALWAYS AS (octet_length(content)) STORED,
    sha256 text NOT NULL CHECK (sha256=encode(sha256(content),'hex')),
    version bigint NOT NULL DEFAULT 1 CHECK (version>0),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    archived_at timestamptz,
    archived_by uuid REFERENCES users(id),
    archive_reason text CHECK (char_length(archive_reason) BETWEEN 3 AND 1000 AND archive_reason !~ '[[:cntrl:]]'),
    CHECK ((archived_at IS NULL AND archived_by IS NULL AND archive_reason IS NULL) OR (archived_at IS NOT NULL AND archived_by IS NOT NULL AND archive_reason IS NOT NULL)),
    sales_order_id uuid GENERATED ALWAYS AS (CASE WHEN document_type='sales_order' THEN document_id END) STORED REFERENCES sales_orders(id),
    invoice_id uuid GENERATED ALWAYS AS (CASE WHEN document_type='invoice' THEN document_id END) STORED REFERENCES invoices(id),
    payment_id uuid GENERATED ALWAYS AS (CASE WHEN document_type='payment' THEN document_id END) STORED REFERENCES payments(id),
    purchase_order_id uuid GENERATED ALWAYS AS (CASE WHEN document_type='purchase_order' THEN document_id END) STORED REFERENCES purchase_orders(id),
    purchase_receipt_id uuid GENERATED ALWAYS AS (CASE WHEN document_type='purchase_receipt' THEN document_id END) STORED REFERENCES purchase_receipts(id),
    purchase_bill_id uuid GENERATED ALWAYS AS (CASE WHEN document_type='purchase_bill' THEN document_id END) STORED REFERENCES purchase_bills(id),
    supplier_payment_id uuid GENERATED ALWAYS AS (CASE WHEN document_type='supplier_payment' THEN document_id END) STORED REFERENCES supplier_payments(id),
    journal_id uuid GENERATED ALWAYS AS (CASE WHEN document_type='journal' THEN document_id END) STORED REFERENCES journal_entries(id),
    sales_return_id uuid GENERATED ALWAYS AS (CASE WHEN document_type='sales_return' THEN document_id END) STORED REFERENCES sales_returns(id),
    supplier_return_id uuid GENERATED ALWAYS AS (CASE WHEN document_type='supplier_return' THEN document_id END) STORED REFERENCES supplier_returns(id)
);
CREATE INDEX document_attachments_document_idx ON document_attachments(document_type,document_id,created_at,id);
CREATE TABLE document_attachment_events (
    id uuid PRIMARY KEY,
    attachment_id uuid NOT NULL REFERENCES document_attachments(id),
    action text NOT NULL CHECK (action IN ('uploaded','archived','restored')),
    reason text CHECK (char_length(reason) BETWEEN 3 AND 1000 AND reason !~ '[[:cntrl:]]'),
    version bigint NOT NULL CHECK (version>0),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(attachment_id,version),
    CHECK ((action='uploaded' AND version=1 AND reason IS NULL) OR (action IN ('archived','restored') AND version>1 AND reason IS NOT NULL))
);
CREATE FUNCTION guard_document_attachment() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE active_files bigint; active_bytes bigint;
BEGIN
    IF TG_OP='UPDATE' THEN
        IF ROW(NEW.id,NEW.document_type,NEW.document_id,NEW.filename,NEW.media_type,NEW.content,NEW.sha256,NEW.created_by,NEW.created_at)
            IS DISTINCT FROM ROW(OLD.id,OLD.document_type,OLD.document_id,OLD.filename,OLD.media_type,OLD.content,OLD.sha256,OLD.created_by,OLD.created_at)
            OR NEW.version<>OLD.version+1 OR (NEW.archived_at IS NULL)=(OLD.archived_at IS NULL) THEN
            RAISE EXCEPTION 'Attachment content and identity are immutable; archive transitions require a new version' USING ERRCODE='23514';
        END IF;
    ELSE
        IF NEW.version<>1 OR NEW.archived_at IS NOT NULL THEN
            RAISE EXCEPTION 'New attachment must be active at version 1' USING ERRCODE='23514';
        END IF;
    END IF;
    IF NEW.archived_at IS NULL THEN
        PERFORM pg_advisory_xact_lock(hashtextextended('attachment:'||NEW.document_type||':'||NEW.document_id::text,0));
        SELECT count(*),COALESCE(sum(byte_size),0) INTO active_files,active_bytes
          FROM document_attachments WHERE document_type=NEW.document_type AND document_id=NEW.document_id AND archived_at IS NULL AND id<>NEW.id;
        IF active_files>=20 OR active_bytes+octet_length(NEW.content)>52428800 THEN
            RAISE EXCEPTION 'Active attachment quota exceeded' USING ERRCODE='23514';
        END IF;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER document_attachment_guard BEFORE INSERT OR UPDATE ON document_attachments FOR EACH ROW EXECUTE FUNCTION guard_document_attachment();
CREATE TRIGGER document_attachment_delete BEFORE DELETE ON document_attachments FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER document_attachment_truncate BEFORE TRUNCATE ON document_attachments FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER document_attachment_event_immutable BEFORE UPDATE OR DELETE ON document_attachment_events FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER document_attachment_event_truncate BEFORE TRUNCATE ON document_attachment_events FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();

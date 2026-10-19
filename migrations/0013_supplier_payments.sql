ALTER TABLE purchase_bills ADD COLUMN paid numeric(18,2) NOT NULL DEFAULT 0 CHECK(paid>=0 AND paid<=total);
ALTER TABLE purchase_bills DROP CONSTRAINT purchase_bills_status_check;
ALTER TABLE purchase_bills ADD CONSTRAINT purchase_bills_status_check CHECK(status IN ('unpaid','partial','paid','void'));
ALTER TABLE purchase_bills ADD CONSTRAINT purchase_bills_paid_status_check CHECK(
    (status='unpaid' AND paid=0) OR (status='partial' AND paid>0 AND paid<total)
    OR (status='paid' AND paid=total) OR (status='void' AND paid=0));
DROP INDEX purchase_bills_active_receipt_idx;
CREATE UNIQUE INDEX purchase_bills_active_receipt_idx ON purchase_bills(receipt_id) WHERE status<>'void';
DROP INDEX purchase_bills_active_invoice_idx;
CREATE UNIQUE INDEX purchase_bills_active_invoice_idx ON purchase_bills(supplier_id,lower(btrim(supplier_invoice_number))) WHERE status<>'void';
CREATE OR REPLACE FUNCTION guard_purchase_bill_update() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.status='void' OR NEW.version<>OLD.version+1
        OR (to_jsonb(NEW)-'status'-'paid'-'version') IS DISTINCT FROM (to_jsonb(OLD)-'status'-'paid'-'version')
        OR (NEW.status='void' AND OLD.paid<>0) THEN
        RAISE EXCEPTION 'Posted bill financial details are immutable; correct dependent payments first';
    END IF;
    RETURN NEW;
END $$;

ALTER TABLE journal_entries DROP CONSTRAINT journal_entries_source_type_check;
ALTER TABLE journal_entries ADD CONSTRAINT journal_entries_source_type_check CHECK(source_type IN
    ('receipt','shipment','invoice','payment','manual','adjustment','reversal','opening','purchase_receipt','purchase_bill','supplier_payment'));
ALTER TABLE posting_reversals DROP CONSTRAINT posting_reversals_source_type_check;
ALTER TABLE posting_reversals ADD CONSTRAINT posting_reversals_source_type_check CHECK(source_type IN
    ('payment','invoice','shipment','opening','manual','receipt','purchase_receipt','purchase_bill','supplier_payment'));
CREATE SEQUENCE supplier_payment_number;
CREATE TABLE supplier_payments(
    id uuid PRIMARY KEY,
    number text NOT NULL UNIQUE,
    bill_id uuid NOT NULL REFERENCES purchase_bills(id),
    amount numeric(18,2) NOT NULL CHECK(amount>0 AND amount<=1000000000000),
    reference text CHECK(reference IS NULL OR char_length(reference) BETWEEN 1 AND 200),
    journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX supplier_payments_bill_idx ON supplier_payments(bill_id,created_at,id);
CREATE TRIGGER immutable_supplier_payments BEFORE UPDATE OR DELETE ON supplier_payments FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_supplier_payments_truncate BEFORE TRUNCATE ON supplier_payments FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();

-- Keep all non-void bills linked to their receipt, including settled bills.
DO $$
DECLARE name text;
BEGIN
    FOREACH name IN ARRAY ARRAY['purchase_receipt_bill_blocked_reason','purchase_receipt_blocked_reason','purchase_receipt_history'] LOOP
        EXECUTE replace(pg_get_functiondef((name||'(uuid)')::regprocedure),'b.status=''unpaid''','b.status<>''void''');
    END LOOP;
END $$;
CREATE OR REPLACE FUNCTION purchase_bill_blocked_reason(bill uuid) RETURNS text LANGUAGE sql STABLE AS $$
    SELECT CASE WHEN b.status='void' THEN 'Tagihan supplier sudah dibalik.'
        WHEN b.paid>0 THEN 'Balikkan seluruh pembayaran aktif sebelum membatalkan tagihan.'
        WHEN NOT (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through
            FROM accounting_period WHERE singleton) THEN 'Periode akuntansi saat ini sudah ditutup.' ELSE NULL END
    FROM purchase_bills b WHERE b.id=bill
$$;

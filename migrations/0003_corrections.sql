ALTER TABLE sales_orders DROP CONSTRAINT sales_orders_status_check;
ALTER TABLE sales_orders ADD CONSTRAINT sales_orders_status_check
    CHECK (status IN ('draft','confirmed','shipped','invoiced','cancelled'));
ALTER TABLE invoices DROP CONSTRAINT invoices_status_check;
ALTER TABLE invoices ADD CONSTRAINT invoices_status_check CHECK (status IN ('unpaid','partial','paid','void'));
ALTER TABLE invoices DROP CONSTRAINT invoices_order_id_key;
CREATE UNIQUE INDEX invoices_active_order_idx ON invoices(order_id) WHERE status <> 'void';

ALTER TABLE journal_entries DROP CONSTRAINT journal_entries_source_type_check;
ALTER TABLE journal_entries ADD CONSTRAINT journal_entries_source_type_check
    CHECK (source_type IN ('receipt','shipment','invoice','payment','manual','adjustment','reversal'));
ALTER TABLE inventory_movements DROP CONSTRAINT inventory_movements_kind_check;
ALTER TABLE inventory_movements DROP CONSTRAINT inventory_movements_check;
ALTER TABLE inventory_movements ADD CONSTRAINT inventory_movements_kind_check
    CHECK (kind IN ('receipt','shipment','adjustment','shipment_reversal'));
ALTER TABLE inventory_movements ADD CONSTRAINT inventory_movements_direction_check CHECK (
    (kind IN ('receipt','shipment_reversal') AND quantity>0) OR
    (kind='shipment' AND quantity<0) OR (kind='adjustment' AND quantity<>0));
ALTER TABLE inventory_movements ADD COLUMN reverses_id uuid UNIQUE REFERENCES inventory_movements(id);

CREATE TABLE shipment_postings (
    id uuid PRIMARY KEY,
    order_id uuid NOT NULL REFERENCES sales_orders(id),
    reversed boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX shipment_active_order_idx ON shipment_postings(order_id) WHERE NOT reversed;
-- Preserve references if this migration is applied to an existing installation.
INSERT INTO shipment_postings(id,order_id)
    SELECT DISTINCT order_id,order_id FROM inventory_movements WHERE kind='shipment';
ALTER TABLE inventory_movements ADD COLUMN shipment_posting_id uuid REFERENCES shipment_postings(id);
-- The immutable ledger trigger is temporarily bypassed only for this migration's backfill.
ALTER TABLE inventory_movements DISABLE TRIGGER immutable_inventory;
UPDATE inventory_movements SET shipment_posting_id=order_id WHERE kind='shipment';
ALTER TABLE inventory_movements ENABLE TRIGGER immutable_inventory;

CREATE TABLE posting_reversals (
    id uuid PRIMARY KEY,
    source_type text NOT NULL CHECK (source_type IN ('payment','invoice','shipment')),
    source_id uuid NOT NULL,
    reason text NOT NULL CHECK (length(reason) BETWEEN 3 AND 1000),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (source_type,source_id)
);
CREATE TRIGGER immutable_reversals BEFORE UPDATE OR DELETE ON posting_reversals
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();

CREATE TABLE accounting_period (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    closed_through date,
    closed_by uuid REFERENCES users(id),
    reason text,
    closed_at timestamptz
);
INSERT INTO accounting_period(singleton) VALUES(true);

CREATE TRIGGER immutable_inventory_truncate BEFORE TRUNCATE ON inventory_movements
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_journal_truncate BEFORE TRUNCATE ON journal_entries
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_journal_lines_truncate BEFORE TRUNCATE ON journal_lines
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_payments_truncate BEFORE TRUNCATE ON payments
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_reversals_truncate BEFORE TRUNCATE ON posting_reversals
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();

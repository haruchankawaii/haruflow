-- Received stock is accrued separately from opening equity and supplier bills.
-- Never adopt a user-created account merely because its code matches our default.
ALTER TABLE accounts ADD COLUMN system_role text UNIQUE;
DO $$
DECLARE accrual_code text;
BEGIN
    SELECT n::text INTO accrual_code FROM generate_series(2110,2199) n
        WHERE NOT EXISTS (SELECT 1 FROM accounts WHERE code=n::text) ORDER BY n LIMIT 1;
    IF accrual_code IS NULL THEN
        WITH candidates AS (
            SELECT 1000000000::bigint n
            UNION
            SELECT code::bigint+1 FROM accounts
                WHERE code ~ '^[1-9][0-9]{9}$' AND code<>'9999999999'
        )
        SELECT min(n)::text INTO accrual_code FROM candidates
            WHERE NOT EXISTS (SELECT 1 FROM accounts WHERE code=n::text);
    END IF;
    IF accrual_code IS NULL THEN RAISE EXCEPTION 'No unused goods receipt accrual account code'; END IF;
    INSERT INTO accounts(code,name,kind,system_role)
        VALUES(accrual_code,'Barang diterima belum ditagih','liability','purchase_receipt_accrual');
END $$;

ALTER TABLE journal_entries DROP CONSTRAINT journal_entries_source_type_check;
ALTER TABLE journal_entries ADD CONSTRAINT journal_entries_source_type_check
    CHECK (source_type IN ('receipt','shipment','invoice','payment','manual','adjustment','reversal','opening','purchase_receipt'));
ALTER TABLE posting_reversals DROP CONSTRAINT posting_reversals_source_type_check;
ALTER TABLE posting_reversals ADD CONSTRAINT posting_reversals_source_type_check
    CHECK (source_type IN ('payment','invoice','shipment','opening','manual','receipt','purchase_receipt'));

CREATE SEQUENCE purchase_receipt_number;
CREATE TABLE purchase_receipts (
    id uuid PRIMARY KEY,
    number text NOT NULL UNIQUE,
    order_id uuid NOT NULL REFERENCES purchase_orders(id),
    reference text CHECK (reference IS NULL OR (char_length(reference) BETWEEN 1 AND 200 AND reference !~ '[[:cntrl:]]')),
    total numeric(18,2) NOT NULL CHECK (total>0 AND total<=1000000000000),
    journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    posting_transaction xid8 NOT NULL DEFAULT pg_current_xact_id(),
    UNIQUE(id,order_id)
);
CREATE INDEX purchase_receipts_order_idx ON purchase_receipts(order_id,created_at,id);
CREATE TRIGGER immutable_purchase_receipts BEFORE UPDATE OR DELETE ON purchase_receipts
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_purchase_receipts_truncate BEFORE TRUNCATE ON purchase_receipts
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();

ALTER TABLE purchase_orders ADD COLUMN received_at timestamptz;
ALTER TABLE purchase_orders ADD COLUMN active_receipt_id uuid UNIQUE;
ALTER TABLE purchase_orders ADD CONSTRAINT purchase_active_receipt_order_fk
    FOREIGN KEY(active_receipt_id,id) REFERENCES purchase_receipts(id,order_id);
ALTER TABLE purchase_orders DROP CONSTRAINT purchase_orders_status_check;
ALTER TABLE purchase_orders ADD CONSTRAINT purchase_orders_status_check CHECK (status IN ('draft','confirmed','received','cancelled'));
ALTER TABLE purchase_orders DROP CONSTRAINT purchase_orders_check;
ALTER TABLE purchase_orders ADD CONSTRAINT purchase_orders_state_check CHECK (
    (status='draft' AND confirmed_at IS NULL AND cancelled_at IS NULL AND cancellation_reason IS NULL AND received_at IS NULL AND active_receipt_id IS NULL) OR
    (status='confirmed' AND confirmed_at IS NOT NULL AND cancelled_at IS NULL AND cancellation_reason IS NULL AND received_at IS NULL AND active_receipt_id IS NULL) OR
    (status='received' AND confirmed_at IS NOT NULL AND cancelled_at IS NULL AND cancellation_reason IS NULL AND received_at IS NOT NULL AND active_receipt_id IS NOT NULL) OR
    (status='cancelled' AND cancelled_at IS NOT NULL AND cancellation_reason IS NOT NULL AND received_at IS NULL AND active_receipt_id IS NULL));

ALTER TABLE inventory_movements ADD COLUMN purchase_receipt_id uuid REFERENCES purchase_receipts(id);
ALTER TABLE inventory_movements DROP CONSTRAINT inventory_movements_kind_check;
ALTER TABLE inventory_movements DROP CONSTRAINT inventory_movements_direction_check;
ALTER TABLE inventory_movements ADD CONSTRAINT inventory_movements_kind_check
    CHECK (kind IN ('receipt','shipment','adjustment','shipment_reversal','opening','opening_reversal','receipt_reversal','purchase_receipt','purchase_receipt_reversal'));
ALTER TABLE inventory_movements ADD CONSTRAINT inventory_movements_direction_check CHECK (
    (kind IN ('receipt','shipment_reversal','opening','purchase_receipt') AND quantity>0) OR
    (kind IN ('shipment','opening_reversal','receipt_reversal','purchase_receipt_reversal') AND quantity<0) OR
    (kind='adjustment' AND quantity<>0));
ALTER TABLE inventory_movements ADD CONSTRAINT purchase_receipt_reference_check CHECK (
    (kind IN ('purchase_receipt','purchase_receipt_reversal') AND purchase_receipt_id IS NOT NULL) OR
    (kind NOT IN ('purchase_receipt','purchase_receipt_reversal') AND purchase_receipt_id IS NULL));
ALTER TABLE inventory_movements ADD CONSTRAINT purchase_receipt_reversal_reference_check
    CHECK (kind<>'purchase_receipt_reversal' OR reverses_id IS NOT NULL);
CREATE UNIQUE INDEX purchase_receipt_product_movement_idx ON inventory_movements(purchase_receipt_id,product_id)
    WHERE kind='purchase_receipt';

CREATE TABLE purchase_receipt_lines (
    id uuid PRIMARY KEY,
    receipt_id uuid NOT NULL REFERENCES purchase_receipts(id),
    position integer NOT NULL CHECK (position BETWEEN 1 AND 100),
    movement_id uuid NOT NULL UNIQUE REFERENCES inventory_movements(id),
    product_id uuid NOT NULL REFERENCES products(id),
    product_sku text NOT NULL,
    product_name text NOT NULL,
    unit text NOT NULL,
    quantity numeric(18,3) NOT NULL CHECK (quantity>0 AND quantity<=1000000000),
    unit_cost numeric(18,4) NOT NULL CHECK (unit_cost>0 AND unit_cost<=1000000000),
    value numeric(18,2) NOT NULL CHECK (value>0 AND value<=1000000000000),
    before_quantity numeric(18,3) NOT NULL CHECK (before_quantity>=0),
    before_average_cost numeric(18,4) NOT NULL CHECK (before_average_cost>=0),
    before_inventory_value numeric(18,2) NOT NULL CHECK (before_inventory_value>=0),
    CHECK (before_quantity>0 OR before_inventory_value=0),
    UNIQUE(receipt_id,position),
    UNIQUE(receipt_id,product_id)
);
CREATE TRIGGER immutable_purchase_receipt_lines BEFORE UPDATE OR DELETE ON purchase_receipt_lines
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_purchase_receipt_lines_truncate BEFORE TRUNCATE ON purchase_receipt_lines
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE FUNCTION guard_purchase_receipt_append() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE current_transaction xid8 := pg_current_xact_id();
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM purchase_receipts WHERE id=NEW.receipt_id
            AND posting_transaction=current_transaction
            AND xmin::text::numeric=mod(current_transaction::text::numeric,4294967296)
    ) THEN
        RAISE EXCEPTION 'Cannot append lines to a posted purchase receipt';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER guard_purchase_receipt_lines BEFORE INSERT ON purchase_receipt_lines
    FOR EACH ROW EXECUTE FUNCTION guard_purchase_receipt_append();

-- All read endpoints use these functions inside their single statement snapshot.
-- Negative later movements, including reversals, still block historical rewinds.
CREATE FUNCTION purchase_receipt_blocked_reason(receipt uuid) RETURNS text LANGUAGE sql STABLE AS $$
    SELECT CASE
        WHEN EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='purchase_receipt' AND source_id=r.id)
            THEN 'Penerimaan PO sudah dibalik.'
        WHEN o.status<>'received' OR o.active_receipt_id IS DISTINCT FROM r.id
            THEN 'Penerimaan ini bukan penerimaan aktif PO.'
        WHEN EXISTS(SELECT 1 FROM purchase_receipt_lines l JOIN inventory_movements m ON m.id=l.movement_id
            WHERE l.receipt_id=r.id AND EXISTS(SELECT 1 FROM inventory_movements later
                WHERE later.product_id=l.product_id AND later.movement_seq>m.movement_seq))
            THEN 'Ada mutasi stok setelah penerimaan ini; koreksi tidak dapat memutar ulang HPP historis.'
        WHEN EXISTS(SELECT 1 FROM purchase_receipt_lines l JOIN products p ON p.id=l.product_id
            WHERE l.receipt_id=r.id AND (p.quantity<>l.before_quantity+l.quantity
                OR p.inventory_value<>l.before_inventory_value+l.value
                OR p.average_cost<>round((l.before_inventory_value+l.value)/(l.before_quantity+l.quantity),4)))
            THEN 'Saldo stok tidak lagi cocok dengan snapshot penerimaan.'
        WHEN NOT (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through
            FROM accounting_period WHERE singleton)
            THEN 'Periode akuntansi saat ini sudah ditutup.'
        ELSE NULL END
    FROM purchase_receipts r JOIN purchase_orders o ON o.id=r.order_id WHERE r.id=receipt
$$;

CREATE FUNCTION purchase_receipt_history(purchase_order uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'id',r.id,'number',r.number,'order_id',r.order_id,'reference',r.reference,'total',r.total::text,
        'journal_id',r.journal_id,'created_at',r.created_at,
        'can_reverse',purchase_receipt_blocked_reason(r.id) IS NULL,
        'reversal_blocked_reason',purchase_receipt_blocked_reason(r.id),
        'reversal',CASE WHEN rv.id IS NULL THEN NULL ELSE jsonb_build_object(
            'id',rv.id,'reason',rv.reason,'created_at',rv.created_at,'journal_id',rj.id) END,
        'items',COALESCE((SELECT jsonb_agg(jsonb_build_object(
            'id',l.id,'movement_id',l.movement_id,'product_id',l.product_id,'product_sku',l.product_sku,
            'product_name',l.product_name,'unit',l.unit,'quantity',l.quantity::text,'unit_cost',l.unit_cost::text,'value',l.value::text)
            ORDER BY l.position) FROM purchase_receipt_lines l WHERE l.receipt_id=r.id),'[]'::jsonb)
        ) ORDER BY r.created_at DESC,r.number DESC),'[]'::jsonb)
    FROM purchase_receipts r
    LEFT JOIN posting_reversals rv ON rv.source_type='purchase_receipt' AND rv.source_id=r.id
    LEFT JOIN journal_entries rj ON rj.source_type='reversal' AND rj.source_id=rv.id
    WHERE r.order_id=purchase_order
$$;

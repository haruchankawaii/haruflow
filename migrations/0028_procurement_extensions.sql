-- Keep existing posted values unchanged. New procurement snapshots are immutable.
DO $$ DECLARE role_name text; account_kind text; account_name text; chosen text; BEGIN
FOR role_name,account_kind,account_name IN VALUES
 ('purchase_service_expense','expense','Jasa pembelian'),
 ('purchase_price_variance','expense','Selisih harga tagihan pembelian'),
 ('purchase_shipping_expense','expense','Ongkir pembelian'),
 ('purchase_discount','expense','Diskon pembelian'),
 ('purchase_input_tax','asset','Pajak masukan pembelian') LOOP
 SELECT n::text INTO chosen FROM generate_series(6000,6999) n WHERE NOT EXISTS(SELECT 1 FROM accounts WHERE code=n::text) ORDER BY n LIMIT 1;
 IF chosen IS NULL THEN SELECT n::text INTO chosen FROM generate_series(1000000000::bigint,1000010000::bigint) n WHERE NOT EXISTS(SELECT 1 FROM accounts WHERE code=n::text) ORDER BY n LIMIT 1; END IF;
 IF chosen IS NULL THEN RAISE EXCEPTION 'No unused procurement control account'; END IF;
 INSERT INTO accounts(code,name,kind,system_role) VALUES(chosen,account_name,account_kind,role_name);
END LOOP; END $$;
ALTER TABLE purchase_order_lines ADD COLUMN product_kind text NOT NULL DEFAULT 'stock' CHECK(product_kind IN ('stock','service'));
ALTER TABLE purchase_receipt_lines ADD COLUMN product_kind text NOT NULL DEFAULT 'stock' CHECK(product_kind IN ('stock','service'));
ALTER TABLE purchase_receipt_lines ALTER COLUMN movement_id DROP NOT NULL;
ALTER TABLE purchase_receipt_lines ADD CONSTRAINT receipt_kind_movement CHECK((product_kind='stock')=(movement_id IS NOT NULL));
ALTER TABLE purchase_bills ADD COLUMN receipt_value numeric(18,2) NOT NULL DEFAULT 0 CHECK(receipt_value>=0);
ALTER TABLE purchase_bills ADD COLUMN subtotal numeric(18,2) NOT NULL DEFAULT 0 CHECK(subtotal>=0);
ALTER TABLE purchase_bills ADD COLUMN shipping numeric(18,2) NOT NULL DEFAULT 0 CHECK(shipping>=0);
ALTER TABLE purchase_bills ADD COLUMN discount numeric(18,2) NOT NULL DEFAULT 0 CHECK(discount>=0);
ALTER TABLE purchase_bills ADD COLUMN tax_value numeric(18,2) NOT NULL DEFAULT 0 CHECK(tax_value>=0);
ALTER TABLE purchase_bills ADD COLUMN tax_snapshot jsonb;
ALTER TABLE purchase_bills DISABLE TRIGGER guard_purchase_bill_update;
UPDATE purchase_bills SET receipt_value=total,subtotal=total;
ALTER TABLE purchase_bills ENABLE TRIGGER guard_purchase_bill_update;
ALTER TABLE purchase_bill_lines ADD COLUMN receipt_value numeric(18,2) NOT NULL DEFAULT 0 CHECK(receipt_value>=0);
ALTER TABLE purchase_bill_lines ADD COLUMN tax_value numeric(18,2) NOT NULL DEFAULT 0 CHECK(tax_value>=0 AND tax_value<=value);
ALTER TABLE purchase_bill_lines ADD COLUMN product_kind text NOT NULL DEFAULT 'stock' CHECK(product_kind IN ('stock','service'));
ALTER TABLE purchase_bill_lines DISABLE TRIGGER immutable_purchase_bill_lines;
UPDATE purchase_bill_lines SET receipt_value=value;
ALTER TABLE purchase_bill_lines ENABLE TRIGGER immutable_purchase_bill_lines;
ALTER TABLE supplier_return_lines ALTER COLUMN movement_id DROP NOT NULL;
ALTER TABLE supplier_return_lines ADD COLUMN product_kind text NOT NULL DEFAULT 'stock' CHECK(product_kind IN ('stock','service'));
ALTER TABLE supplier_return_lines ADD COLUMN tax_value numeric(18,2) NOT NULL DEFAULT 0 CHECK(tax_value>=0 AND tax_value<=credit);
ALTER TABLE supplier_return_lines ADD CONSTRAINT supplier_return_kind_movement CHECK((product_kind='stock')=(movement_id IS NOT NULL));
CREATE OR REPLACE FUNCTION purchase_receipt_billed_value(receipt_line uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
 SELECT COALESCE(sum(l.receipt_value),0) FROM purchase_bill_lines l JOIN purchase_bills b ON b.id=l.bill_id WHERE l.receipt_line_id=receipt_line AND b.status<>'void'
$$;
CREATE FUNCTION supplier_bill_returned_tax(bill_line uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
 SELECT COALESCE(sum(l.tax_value),0) FROM supplier_return_lines l JOIN supplier_returns r ON r.id=l.return_id WHERE l.bill_line_id=bill_line AND NOT EXISTS(SELECT 1 FROM posting_reversals v WHERE v.source_type='supplier_return' AND v.source_id=r.id)
$$;
CREATE OR REPLACE FUNCTION purchase_receipt_blocked_reason(receipt uuid) RETURNS text LANGUAGE sql STABLE AS $$
    SELECT CASE
        WHEN EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='purchase_receipt' AND source_id=r.id)
            THEN 'Penerimaan PO sudah dibalik.'
        WHEN o.status NOT IN ('partial_received','received') THEN 'PO tidak memiliki penerimaan aktif.'
        WHEN EXISTS(SELECT 1 FROM purchase_bills b WHERE b.receipt_id=r.id AND b.status<>'void')
            THEN 'Balikkan tagihan supplier aktif sebelum membalik penerimaan ini.'
        WHEN EXISTS(SELECT 1 FROM purchase_receipt_lines l JOIN inventory_movements m ON m.id=l.movement_id
            WHERE l.receipt_id=r.id AND EXISTS(SELECT 1 FROM inventory_movements later
                WHERE later.product_id=l.product_id AND later.movement_seq>m.movement_seq))
            THEN 'Ada mutasi stok setelah penerimaan ini; koreksi tidak dapat memutar ulang HPP historis.'
        WHEN EXISTS(SELECT 1 FROM purchase_receipt_lines l JOIN products p ON p.id=l.product_id
            WHERE l.receipt_id=r.id AND l.product_kind='stock' AND (p.quantity<>l.before_quantity+l.quantity
                OR p.inventory_value<>l.before_inventory_value+l.value
                OR p.average_cost<>round((l.before_inventory_value+l.value)/(l.before_quantity+l.quantity),4)))
            THEN 'Saldo stok tidak lagi cocok dengan snapshot penerimaan.'
        WHEN NOT (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through
            FROM accounting_period WHERE singleton) THEN 'Periode akuntansi saat ini sudah ditutup.'
        ELSE NULL END
    FROM purchase_receipts r JOIN purchase_orders o ON o.id=r.order_id WHERE r.id=receipt
$$;
CREATE OR REPLACE FUNCTION purchase_receipt_history(purchase_order uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'id',r.id,'number',r.number,'order_id',r.order_id,'reference',r.reference,'total',r.total::text,
        'journal_id',r.journal_id,'created_at',r.created_at,
        'can_reverse',purchase_receipt_blocked_reason(r.id) IS NULL,
        'reversal_blocked_reason',purchase_receipt_blocked_reason(r.id),
        'bill_id',(SELECT b.id FROM purchase_bills b WHERE b.receipt_id=r.id AND b.status<>'void' ORDER BY b.created_at,b.id LIMIT 1),
        'bill_number',(SELECT b.number FROM purchase_bills b WHERE b.receipt_id=r.id AND b.status<>'void' ORDER BY b.created_at,b.id LIMIT 1),
        'bills',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',b.id,'number',b.number,'status',b.status,'total',b.total::text)
            ORDER BY b.created_at,b.id) FROM purchase_bills b WHERE b.receipt_id=r.id AND b.status<>'void'),'[]'::jsonb),
        'can_bill',purchase_receipt_bill_blocked_reason(r.id) IS NULL,
        'bill_blocked_reason',purchase_receipt_bill_blocked_reason(r.id),
        'reversal',CASE WHEN rv.id IS NULL THEN NULL ELSE jsonb_build_object(
            'id',rv.id,'reason',rv.reason,'created_at',rv.created_at,'journal_id',rj.id) END,
        'items',COALESCE((SELECT jsonb_agg(jsonb_build_object(
            'id',l.id,'movement_id',l.movement_id,'product_id',l.product_id,'product_sku',l.product_sku,
            'product_name',l.product_name,'product_kind',l.product_kind,'unit',l.unit,'quantity',l.quantity::text,'unit_cost',l.unit_cost::text,'value',l.value::text,
            'billed_quantity',purchase_receipt_billed_quantity(l.id)::text,
            'remaining_bill_quantity',(l.quantity-purchase_receipt_billed_quantity(l.id))::text,
            'billed_value',purchase_receipt_billed_value(l.id)::text)
            ORDER BY l.position) FROM purchase_receipt_lines l WHERE l.receipt_id=r.id),'[]'::jsonb)
        ) ORDER BY r.created_at DESC,r.number DESC),'[]'::jsonb)
    FROM purchase_receipts r
    LEFT JOIN posting_reversals rv ON rv.source_type='purchase_receipt' AND rv.source_id=r.id
    LEFT JOIN journal_entries rj ON rj.source_type='reversal' AND rj.source_id=rv.id
    WHERE r.order_id=purchase_order
$$;

ALTER FUNCTION supplier_return_blocked_reason(uuid) RENAME TO supplier_return_stock_blocked_legacy;
CREATE FUNCTION supplier_return_blocked_reason(target uuid) RETURNS text LANGUAGE sql STABLE AS $$
 SELECT CASE
 WHEN EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='supplier_return' AND source_id=r.id) THEN 'Retur supplier sudah dibalik.'
 WHEN b.status='void' THEN 'Tagihan supplier tidak aktif.'
 WHEN return_settlement_blocked('supplier_return',target) IS NOT NULL THEN return_settlement_blocked('supplier_return',target)
 WHEN EXISTS(SELECT 1 FROM supplier_return_lines l JOIN inventory_movements m ON m.id=l.movement_id WHERE l.return_id=r.id AND EXISTS(SELECT 1 FROM inventory_movements later WHERE later.product_id=l.product_id AND later.movement_seq>m.movement_seq)) THEN 'Ada mutasi stok setelah retur; saldo historis tidak dapat diputar ulang.'
 WHEN EXISTS(SELECT 1 FROM supplier_return_lines l JOIN products p ON p.id=l.product_id WHERE l.return_id=r.id AND l.product_kind='stock' AND (p.quantity<>l.before_quantity-l.quantity OR p.inventory_value<>l.before_value-l.value OR p.average_cost<>CASE WHEN l.before_quantity=l.quantity THEN 0 ELSE l.before_average END)) THEN 'Saldo stok tidak cocok dengan snapshot retur.'
 WHEN r.refund>COALESCE((SELECT sum(debit-credit) FROM journal_lines WHERE account_code=(SELECT l.account_code FROM journal_lines l JOIN treasury_accounts t ON t.account_code=l.account_code WHERE l.journal_id=r.journal_id LIMIT 1)),0) THEN 'Saldo kas/bank asal tidak cukup untuk membalik pengembalian supplier.'
 WHEN NOT(SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through FROM accounting_period WHERE singleton) THEN 'Periode akuntansi saat ini sudah ditutup.'
 ELSE NULL END FROM supplier_returns r JOIN purchase_bills b ON b.id=r.bill_id WHERE r.id=target
$$;

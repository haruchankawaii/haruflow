-- Landed costs are explicit reclassifications. No historical purchase or stock
-- posting is rewritten, and a cost can only be capitalized once while active.
DO $$ DECLARE expression text; BEGIN
 SELECT pg_get_expr(conbin,conrelid) INTO expression FROM pg_constraint WHERE conrelid='journal_entries'::regclass AND conname='journal_entries_source_type_check';
 ALTER TABLE journal_entries DROP CONSTRAINT journal_entries_source_type_check;
 EXECUTE format('ALTER TABLE journal_entries ADD CONSTRAINT journal_entries_source_type_check CHECK ((%s) OR source_type=''landed_cost'')',expression);
 SELECT pg_get_expr(conbin,conrelid) INTO expression FROM pg_constraint WHERE conrelid='posting_reversals'::regclass AND conname='posting_reversals_source_type_check';
 ALTER TABLE posting_reversals DROP CONSTRAINT posting_reversals_source_type_check;
 EXECUTE format('ALTER TABLE posting_reversals ADD CONSTRAINT posting_reversals_source_type_check CHECK ((%s) OR source_type=''landed_cost'')',expression);
END $$;
CREATE SEQUENCE landed_cost_number;
CREATE TABLE landed_costs(
 id uuid PRIMARY KEY,number text NOT NULL UNIQUE,
 source_line_id uuid NOT NULL REFERENCES journal_lines(id),
 source_journal_id uuid NOT NULL REFERENCES journal_entries(id),
 source_type text NOT NULL CHECK(source_type IN ('purchase_bill','purchase_receipt')),
 source_id uuid NOT NULL,source_number text NOT NULL,source_account text NOT NULL REFERENCES accounts(code),
 allocation_method text NOT NULL CHECK(allocation_method IN ('value','quantity')),
 amount numeric(18,2) NOT NULL CHECK(amount>0 AND amount<=1000000000000),
 inventory_amount numeric(18,2) NOT NULL CHECK(inventory_amount>=0),expense_amount numeric(18,2) NOT NULL CHECK(expense_amount>=0),
 reference text CHECK(char_length(reference) BETWEEN 1 AND 200),
 reason text NOT NULL CHECK(char_length(reason) BETWEEN 3 AND 1000),
 journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),
 posting_transaction xid8 NOT NULL DEFAULT pg_current_xact_id(),
 created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now(),
 CHECK(amount=inventory_amount+expense_amount));
CREATE INDEX landed_cost_source_idx ON landed_costs(source_line_id);
CREATE TABLE landed_cost_lines(
 id uuid PRIMARY KEY,landed_cost_id uuid NOT NULL REFERENCES landed_costs(id),position integer NOT NULL CHECK(position>0),
 receipt_line_id uuid NOT NULL REFERENCES purchase_receipt_lines(id),product_id uuid NOT NULL REFERENCES products(id),
 product_sku text NOT NULL,product_name text NOT NULL,unit text NOT NULL,receipt_number text NOT NULL,
 quantity numeric(18,3) NOT NULL CHECK(quantity>0),receipt_value numeric(18,2) NOT NULL CHECK(receipt_value>0),
 weight numeric(18,3) NOT NULL CHECK(weight>0),remaining_ratio numeric(30,28) NOT NULL CHECK(remaining_ratio BETWEEN 0 AND 1),
 allocated numeric(18,2) NOT NULL CHECK(allocated>=0),inventory_amount numeric(18,2) NOT NULL CHECK(inventory_amount>=0),expense_amount numeric(18,2) NOT NULL CHECK(expense_amount>=0),
 before_quantity numeric(18,3) NOT NULL CHECK(before_quantity>=0),before_value numeric(18,2) NOT NULL CHECK(before_value>=0),before_average numeric(18,4) NOT NULL CHECK(before_average>=0),
 after_value numeric(18,2) NOT NULL CHECK(after_value>=0),after_average numeric(18,4) NOT NULL CHECK(after_average>=0),
 receipt_movement_seq bigint NOT NULL,cutoff_movement_seq bigint NOT NULL,cutoff_location_seq bigint NOT NULL DEFAULT 0,
 CHECK(allocated=inventory_amount+expense_amount),CHECK(after_value=before_value+inventory_amount),
 CHECK(cutoff_movement_seq>=receipt_movement_seq),CHECK(before_quantity>0 OR inventory_amount=0),
 UNIQUE(landed_cost_id,position),UNIQUE(landed_cost_id,product_id),UNIQUE(landed_cost_id,receipt_line_id));
CREATE INDEX landed_cost_product_idx ON landed_cost_lines(product_id,cutoff_movement_seq);
CREATE TABLE landed_cost_location_events(
 landed_cost_id uuid NOT NULL REFERENCES landed_costs(id),event_id uuid PRIMARY KEY REFERENCES stock_location_events(id),
 phase text NOT NULL CHECK(phase IN ('post','reverse')));

CREATE FUNCTION landed_cost_source_used(target uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
 SELECT COALESCE(sum(c.amount),0) FROM landed_costs c WHERE c.source_line_id=target
 AND NOT EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='landed_cost' AND r.source_id=c.id)
$$;
CREATE FUNCTION landed_cost_source_blocked_reason(target uuid) RETURNS text LANGUAGE sql STABLE AS $$
 SELECT CASE
 WHEN l.debit<=0 OR l.credit<>0 OR a.kind<>'expense' OR NOT(a.system_role IN ('purchase_shipping_expense','purchase_price_variance','purchase_service_expense')) THEN 'Pilih biaya ongkir, selisih harga positif, atau jasa pembelian yang sudah dibukukan.'
 WHEN NOT((j.source_type='purchase_bill' AND a.system_role IN ('purchase_shipping_expense','purchase_price_variance')) OR (j.source_type='purchase_receipt' AND a.system_role='purchase_service_expense')) THEN 'Sumber biaya tidak sesuai dengan dokumen pembelian.'
 WHEN j.source_type='purchase_bill' AND NOT EXISTS(SELECT 1 FROM purchase_bills b WHERE b.id=j.source_id AND b.status<>'void') THEN 'Tagihan sumber biaya sudah dibalik.'
 WHEN j.source_type='purchase_receipt' AND EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='purchase_receipt' AND r.source_id=j.source_id) THEN 'Penerimaan sumber biaya sudah dibalik.'
 WHEN j.source_type='purchase_bill' AND EXISTS(SELECT 1 FROM purchase_bills b WHERE b.id=j.source_id AND (b.discount>0 OR (b.tax_value>0 AND b.tax_snapshot->>'basis'='inclusive'))) THEN 'Biaya pada tagihan dengan diskon atau pajak inklusif memerlukan rincian biaya neto terpisah sebelum kapitalisasi.'
 WHEN j.source_type='purchase_receipt' AND (NOT EXISTS(SELECT 1 FROM purchase_bills b WHERE b.receipt_id=j.source_id AND b.status<>'void') OR EXISTS(SELECT 1 FROM purchase_receipt_lines r WHERE r.receipt_id=j.source_id AND purchase_receipt_billed_quantity(r.id)<>r.quantity)) THEN 'Tagihkan seluruh penerimaan jasa sebelum mengalokasikan biaya finalnya.'
 WHEN j.source_type='purchase_receipt' AND EXISTS(SELECT 1 FROM purchase_bills b WHERE b.receipt_id=j.source_id AND b.status<>'void' AND (b.subtotal<>b.receipt_value OR b.discount>0 OR (b.tax_value>0 AND b.tax_snapshot->>'basis'='inclusive'))) THEN 'Biaya jasa memerlukan tagihan final tanpa selisih harga, diskon, atau pajak inklusif; gunakan bukti biaya neto terpisah.'
 WHEN EXISTS(SELECT 1 FROM supplier_returns r JOIN purchase_bills b ON b.id=r.bill_id WHERE ((j.source_type='purchase_bill' AND b.id=j.source_id) OR (j.source_type='purchase_receipt' AND b.receipt_id=j.source_id)) AND NOT EXISTS(SELECT 1 FROM posting_reversals v WHERE v.source_type='supplier_return' AND v.source_id=r.id)) THEN 'Sumber biaya memiliki retur aktif; balikkan retur sebelum alokasi biaya.'
 WHEN l.debit<=landed_cost_source_used(l.id) THEN 'Seluruh biaya sumber sudah dialokasikan.'
 ELSE NULL END FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_id JOIN accounts a ON a.code=l.account_code WHERE l.id=target
$$;
CREATE VIEW landed_cost_source_options AS
 SELECT l.id line_id,j.id journal_id,j.source_type,j.source_id,COALESCE(b.number,r.number) number,
 a.code account_code,a.name account_name,a.system_role,l.debit amount,(l.debit-landed_cost_source_used(l.id)) available,
 landed_cost_source_blocked_reason(l.id) blocked_reason,j.created_at
 FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_id JOIN accounts a ON a.code=l.account_code
 LEFT JOIN purchase_bills b ON j.source_type='purchase_bill' AND b.id=j.source_id
 LEFT JOIN purchase_receipts r ON j.source_type='purchase_receipt' AND r.id=j.source_id
 WHERE l.debit>0 AND l.credit=0 AND a.kind='expense'
 AND ((j.source_type='purchase_bill' AND a.system_role IN ('purchase_shipping_expense','purchase_price_variance')) OR (j.source_type='purchase_receipt' AND a.system_role='purchase_service_expense'));
CREATE FUNCTION guard_landed_cost_source() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE source_record record; blocked text;
BEGIN
 PERFORM pg_advisory_xact_lock(8414077);
 PERFORM id FROM journal_lines WHERE id=NEW.source_line_id FOR UPDATE;
 SELECT * INTO source_record FROM landed_cost_source_options WHERE line_id=NEW.source_line_id;
 IF NOT FOUND THEN RAISE EXCEPTION 'Ineligible landed cost source' USING ERRCODE='23514'; END IF;
 blocked:=source_record.blocked_reason;
 IF blocked IS NOT NULL OR NEW.amount>source_record.available OR NEW.source_journal_id<>source_record.journal_id OR NEW.source_type<>source_record.source_type OR NEW.source_id<>source_record.source_id OR NEW.source_account<>source_record.account_code THEN
  RAISE EXCEPTION 'Landed cost source is unavailable or does not match its snapshot' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER landed_cost_source_guard BEFORE INSERT ON landed_costs FOR EACH ROW EXECUTE FUNCTION guard_landed_cost_source();

CREATE FUNCTION landed_cost_target_blocked_reason(target uuid) RETURNS text LANGUAGE sql STABLE AS $$
 SELECT CASE
 WHEN l.product_kind<>'stock' OR p.kind<>'stock' THEN 'Jasa tidak memiliki persediaan untuk alokasi biaya.'
 WHEN l.unit<>p.unit THEN 'Satuan produk berubah dari snapshot penerimaan.'
 WHEN EXISTS(SELECT 1 FROM posting_reversals v WHERE v.source_type='purchase_receipt' AND v.source_id=l.receipt_id) THEN 'Penerimaan tujuan sudah dibalik.'
 WHEN EXISTS(SELECT 1 FROM inventory_movements n WHERE n.product_id=l.product_id AND n.movement_seq>m.movement_seq AND NOT((n.kind='shipment' AND n.quantity<0) OR (n.kind IN ('receipt','purchase_receipt') AND n.quantity>0))) THEN 'Ada penyesuaian, retur, atau pembalikan stok setelah penerimaan; alokasi historis tidak dapat dihitung dengan aman.'
 ELSE NULL END FROM purchase_receipt_lines l JOIN products p ON p.id=l.product_id LEFT JOIN inventory_movements m ON m.id=l.movement_id WHERE l.id=target
$$;
CREATE FUNCTION landed_cost_bill_dependency(target uuid) RETURNS text LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN EXISTS(SELECT 1 FROM landed_costs c JOIN purchase_bills b ON b.id=target WHERE
 ((c.source_type='purchase_bill' AND c.source_id=b.id) OR (c.source_type='purchase_receipt' AND c.source_id=b.receipt_id))
 AND NOT EXISTS(SELECT 1 FROM posting_reversals v WHERE v.source_type='landed_cost' AND v.source_id=c.id)) THEN 'Balikkan alokasi landed cost dari biaya tagihan ini terlebih dahulu.' ELSE NULL END
$$;
CREATE FUNCTION landed_cost_receipt_dependency(target uuid) RETURNS text LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN EXISTS(SELECT 1 FROM landed_costs c WHERE ((c.source_type='purchase_receipt' AND c.source_id=target)
 OR (c.source_type='purchase_bill' AND EXISTS(SELECT 1 FROM purchase_bills b WHERE b.id=c.source_id AND b.receipt_id=target))
 OR EXISTS(SELECT 1 FROM landed_cost_lines l JOIN purchase_receipt_lines r ON r.id=l.receipt_line_id WHERE l.landed_cost_id=c.id AND r.receipt_id=target))
 AND NOT EXISTS(SELECT 1 FROM posting_reversals v WHERE v.source_type='landed_cost' AND v.source_id=c.id)) THEN 'Balikkan landed cost yang memakai penerimaan ini terlebih dahulu.' ELSE NULL END
$$;
CREATE FUNCTION landed_cost_movement_dependency(target uuid) RETURNS text LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN EXISTS(SELECT 1 FROM inventory_movements m JOIN landed_cost_lines l ON l.product_id=m.product_id
 WHERE m.id=target AND m.movement_seq BETWEEN l.receipt_movement_seq AND l.cutoff_movement_seq
 AND NOT EXISTS(SELECT 1 FROM posting_reversals v WHERE v.source_type='landed_cost' AND v.source_id=l.landed_cost_id))
 THEN 'Balikkan landed cost yang memakai riwayat mutasi ini sebelum membalik stok.' ELSE NULL END
$$;
ALTER FUNCTION purchase_bill_blocked_reason(uuid) RENAME TO purchase_bill_before_landed_cost_blocked_reason;
CREATE FUNCTION purchase_bill_blocked_reason(target uuid) RETURNS text LANGUAGE sql STABLE AS $$ SELECT COALESCE(landed_cost_bill_dependency(target),purchase_bill_before_landed_cost_blocked_reason(target)) $$;
ALTER FUNCTION purchase_receipt_blocked_reason(uuid) RENAME TO purchase_receipt_before_landed_cost_blocked_reason;
CREATE FUNCTION purchase_receipt_blocked_reason(target uuid) RETURNS text LANGUAGE sql STABLE AS $$ SELECT COALESCE(landed_cost_receipt_dependency(target),purchase_receipt_before_landed_cost_blocked_reason(target)) $$;
CREATE FUNCTION guard_landed_cost_dependencies() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE blocked text;
BEGIN
 IF TG_TABLE_NAME='supplier_returns' THEN blocked:=landed_cost_bill_dependency(NEW.bill_id);
 ELSIF TG_TABLE_NAME='inventory_movements' THEN
  IF NEW.reverses_id IS NOT NULL THEN blocked:=landed_cost_movement_dependency(NEW.reverses_id); END IF;
 ELSE
  IF NEW.source_type='landed_cost' THEN blocked:=landed_cost_reversal_blocked_reason(NEW.source_id);
  ELSIF NEW.source_type='purchase_bill' THEN blocked:=landed_cost_bill_dependency(NEW.source_id);
  ELSIF NEW.source_type='purchase_receipt' THEN blocked:=landed_cost_receipt_dependency(NEW.source_id);
  ELSIF NEW.source_type='shipment' THEN
   SELECT landed_cost_movement_dependency(m.id) INTO blocked FROM inventory_movements m WHERE m.shipment_posting_id=NEW.source_id AND landed_cost_movement_dependency(m.id) IS NOT NULL LIMIT 1;
  END IF;
 END IF;
 IF blocked IS NOT NULL THEN RAISE EXCEPTION '%',blocked USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER landed_cost_supplier_return_dependency BEFORE INSERT ON supplier_returns FOR EACH ROW EXECUTE FUNCTION guard_landed_cost_dependencies();
CREATE TRIGGER landed_cost_movement_dependency BEFORE INSERT ON inventory_movements FOR EACH ROW EXECUTE FUNCTION guard_landed_cost_dependencies();
CREATE TRIGGER landed_cost_posting_dependency BEFORE INSERT ON posting_reversals FOR EACH ROW EXECUTE FUNCTION guard_landed_cost_dependencies();
-- Customer returns restore the original shipment cost. A pre-allocation
-- shipment has an additional cost share in the LC journal; returning just its
-- old cost would understate inventory. Post-LC shipments remain returnable.
CREATE FUNCTION guard_landed_cost_sales_return() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE blocked text;
BEGIN
 SELECT landed_cost_movement_dependency(a.movement_id) INTO blocked FROM shipment_allocations a WHERE a.id=NEW.shipment_allocation_id;
 IF blocked IS NOT NULL THEN RAISE EXCEPTION 'Balikkan landed cost terkait sebelum menerima retur dari pengiriman historis ini' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER landed_cost_sales_return_dependency BEFORE INSERT ON sales_return_sources FOR EACH ROW EXECUTE FUNCTION guard_landed_cost_sales_return();

CREATE FUNCTION landed_cost_reversal_blocked_reason(target uuid) RETURNS text LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN EXISTS(SELECT 1 FROM posting_reversals v WHERE v.source_type='landed_cost' AND v.source_id=c.id) THEN 'Landed cost sudah dibalik.'
 WHEN EXISTS(SELECT 1 FROM landed_cost_lines l JOIN products p ON p.id=l.product_id WHERE l.landed_cost_id=c.id AND l.inventory_amount>0 AND
 (p.quantity<>l.before_quantity OR p.inventory_value<>l.after_value OR p.average_cost<>l.after_average
 OR EXISTS(SELECT 1 FROM stock_location_events e WHERE e.product_id=l.product_id AND e.event_seq>l.cutoff_location_seq
 AND NOT EXISTS(SELECT 1 FROM landed_cost_location_events le JOIN posting_reversals v ON v.source_type='landed_cost' AND v.source_id=le.landed_cost_id WHERE le.event_id=e.id))
 OR EXISTS(SELECT 1 FROM inventory_movements m WHERE m.product_id=l.product_id AND m.movement_seq>l.cutoff_movement_seq))) THEN 'Ada mutasi atau perubahan nilai/lokasi stok setelah landed cost; pembalikan tidak boleh mengubah HPP historis.'
 WHEN (SELECT closed_through IS NOT NULL AND (now() AT TIME ZONE (SELECT timezone FROM company))::date<=closed_through FROM accounting_period WHERE singleton) THEN 'Periode akuntansi saat ini sudah ditutup.'
 ELSE NULL END FROM landed_costs c WHERE c.id=target
$$;
CREATE FUNCTION guard_landed_cost_append() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE current_transaction xid8:=pg_current_xact_id();
BEGIN
 IF NOT EXISTS(SELECT 1 FROM landed_costs c WHERE c.id=NEW.landed_cost_id AND c.posting_transaction=current_transaction AND c.xmin::text::numeric=mod(current_transaction::text::numeric,4294967296)) THEN
  -- A reversal may only append valuation links in the transaction inserting
  -- its counterjournal, never modify the original allocation snapshots.
  IF TG_TABLE_NAME<>'landed_cost_location_events' THEN RAISE EXCEPTION 'Cannot append posted landed cost lines' USING ERRCODE='23514'; END IF;
  IF NEW.phase<>'reverse' OR NOT EXISTS(SELECT 1 FROM posting_reversals r JOIN journal_entries j ON j.source_type='reversal' AND j.source_id=r.id WHERE r.source_type='landed_cost' AND r.source_id=NEW.landed_cost_id AND j.posting_transaction=current_transaction AND j.xmin::text::numeric=mod(current_transaction::text::numeric,4294967296)) THEN RAISE EXCEPTION 'Cannot append landed cost valuation links' USING ERRCODE='23514'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER landed_cost_line_append BEFORE INSERT ON landed_cost_lines FOR EACH ROW EXECUTE FUNCTION guard_landed_cost_append();
CREATE TRIGGER landed_cost_location_append BEFORE INSERT ON landed_cost_location_events FOR EACH ROW EXECUTE FUNCTION guard_landed_cost_append();
CREATE FUNCTION verify_landed_cost() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE target uuid; c landed_costs; original journal_lines; inv numeric; exp numeric; total numeric;
BEGIN
 IF TG_TABLE_NAME='landed_costs' THEN target:=NEW.id; ELSE target:=NEW.landed_cost_id; END IF;
 SELECT * INTO c FROM landed_costs WHERE id=target;
 SELECT * INTO original FROM journal_lines WHERE id=c.source_line_id;
 SELECT COALESCE(sum(allocated),0),COALESCE(sum(inventory_amount),0),COALESCE(sum(expense_amount),0) INTO total,inv,exp FROM landed_cost_lines WHERE landed_cost_id=target;
 IF total<>c.amount OR inv<>c.inventory_amount OR exp<>c.expense_amount OR original.journal_id<>c.source_journal_id OR original.account_code<>c.source_account OR landed_cost_source_used(c.source_line_id)>original.debit THEN RAISE EXCEPTION 'Landed cost totals or source capacity do not match' USING ERRCODE='23514'; END IF;
 IF EXISTS(SELECT 1 FROM landed_cost_lines l JOIN purchase_receipt_lines r ON r.id=l.receipt_line_id JOIN inventory_movements m ON m.id=r.movement_id WHERE l.landed_cost_id=target AND
 (l.product_id<>r.product_id OR r.product_kind<>'stock' OR l.quantity<>r.quantity OR l.receipt_value<>r.value OR l.receipt_movement_seq<>m.movement_seq OR l.inventory_amount<>round(l.allocated*l.remaining_ratio,2)
 OR (l.inventory_amount>0 AND l.after_average<>CASE WHEN l.before_quantity=0 THEN 0 ELSE round(l.after_value/l.before_quantity,4) END))) THEN RAISE EXCEPTION 'Landed cost line snapshot does not match its receipt or valuation' USING ERRCODE='23514'; END IF;
 IF NOT EXISTS(SELECT 1 FROM journal_entries WHERE id=c.journal_id AND source_type='landed_cost' AND source_id=c.id)
 OR (SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE journal_id=c.journal_id AND account_code='1300')<>c.inventory_amount
 OR (SELECT COALESCE(sum(debit-credit),0) FROM journal_lines WHERE journal_id=c.journal_id AND account_code='5000')<>c.expense_amount
 OR (SELECT COALESCE(sum(credit-debit),0) FROM journal_lines WHERE journal_id=c.journal_id AND account_code=c.source_account)<>c.amount THEN RAISE EXCEPTION 'Landed cost journal does not match allocation' USING ERRCODE='23514'; END IF;
 IF (SELECT COALESCE(sum(e.value),0) FROM landed_cost_location_events le JOIN stock_location_events e ON e.id=le.event_id WHERE le.landed_cost_id=target AND le.phase='post')<>c.inventory_amount
 OR EXISTS(SELECT 1 FROM landed_cost_location_events le JOIN stock_location_events e ON e.id=le.event_id WHERE le.landed_cost_id=target AND le.phase='post' AND (e.kind<>'valuation' OR e.quantity<>0 OR NOT EXISTS(SELECT 1 FROM landed_cost_lines l WHERE l.landed_cost_id=target AND l.product_id=e.product_id))) THEN RAISE EXCEPTION 'Landed cost location valuations do not match inventory allocation' USING ERRCODE='23514'; END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER landed_cost_totals AFTER INSERT ON landed_costs DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION verify_landed_cost();
CREATE CONSTRAINT TRIGGER landed_cost_line_totals AFTER INSERT ON landed_cost_lines DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION verify_landed_cost();
CREATE CONSTRAINT TRIGGER landed_cost_location_totals AFTER INSERT ON landed_cost_location_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION verify_landed_cost();
DO $$ DECLARE target text; BEGIN
 FOREACH target IN ARRAY ARRAY['landed_costs','landed_cost_lines','landed_cost_location_events'] LOOP
  EXECUTE format('CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION immutable_posting()',target||'_immutable',target);
  EXECUTE format('CREATE TRIGGER %I BEFORE TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting()',target||'_no_truncate',target);
 END LOOP;
END $$;

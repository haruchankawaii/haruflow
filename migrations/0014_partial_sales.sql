-- Allocation records preserve each delivery/invoice independently. Existing full
-- documents are backfilled without changing their inventory or journal postings.
ALTER TABLE sales_orders ADD COLUMN version bigint NOT NULL DEFAULT 1 CHECK(version>0);
DROP INDEX shipment_active_order_idx;
DROP INDEX invoices_active_order_idx;
CREATE INDEX shipment_order_history_idx ON shipment_postings(order_id,created_at,id);
CREATE INDEX invoices_order_history_idx ON invoices(order_id,created_at,id);
ALTER TABLE shipment_postings ADD COLUMN posting_transaction xid8 NOT NULL DEFAULT pg_current_xact_id();
ALTER TABLE invoices ADD COLUMN posting_transaction xid8 NOT NULL DEFAULT pg_current_xact_id();

CREATE TABLE shipment_allocations (
    id uuid PRIMARY KEY,
    shipment_id uuid NOT NULL REFERENCES shipment_postings(id),
    line_id uuid NOT NULL REFERENCES sales_order_lines(id),
    quantity numeric(18,3) NOT NULL CHECK(quantity>0),
    movement_id uuid NOT NULL UNIQUE REFERENCES inventory_movements(id),
    before_quantity numeric(18,3),
    before_value numeric(18,2),
    before_average numeric(18,4),
    UNIQUE(shipment_id,line_id)
);
INSERT INTO shipment_allocations(id,shipment_id,line_id,quantity,movement_id)
    SELECT m.id,m.shipment_posting_id,l.id,-m.quantity,m.id FROM inventory_movements m
    JOIN sales_order_lines l ON l.order_id=m.order_id AND l.product_id=m.product_id
    WHERE m.kind='shipment';
CREATE TABLE invoice_allocations (
    id uuid PRIMARY KEY,
    invoice_id uuid NOT NULL REFERENCES invoices(id),
    line_id uuid NOT NULL REFERENCES sales_order_lines(id),
    quantity numeric(18,3) NOT NULL CHECK(quantity>0),
    total numeric(18,2) NOT NULL CHECK(total>=0),
    UNIQUE(invoice_id,line_id)
);
INSERT INTO invoice_allocations(id,invoice_id,line_id,quantity,total)
    SELECT md5(i.id::text||l.id::text)::uuid,i.id,l.id,l.quantity,l.total
    FROM invoices i JOIN sales_order_lines l ON l.order_id=i.order_id;
CREATE TABLE invoice_shipment_allocations (
    invoice_allocation_id uuid NOT NULL REFERENCES invoice_allocations(id),
    shipment_allocation_id uuid NOT NULL REFERENCES shipment_allocations(id),
    quantity numeric(18,3) NOT NULL CHECK(quantity>0),
    PRIMARY KEY(invoice_allocation_id,shipment_allocation_id)
);
-- An active legacy invoice belongs to its active delivery. For void documents,
-- chronological matching retains the historical delivery where one exists.
INSERT INTO invoice_shipment_allocations(invoice_allocation_id,shipment_allocation_id,quantity)
    SELECT ia.id,sa.id,ia.quantity FROM invoice_allocations ia JOIN invoices i ON i.id=ia.invoice_id
    JOIN LATERAL (
      SELECT s.id FROM shipment_postings s WHERE s.order_id=i.order_id
        AND s.created_at<=i.created_at AND (i.status='void' OR NOT s.reversed)
      ORDER BY s.created_at DESC,s.id DESC LIMIT 1
    ) sh ON true JOIN shipment_allocations sa ON sa.shipment_id=sh.id AND sa.line_id=ia.line_id;

CREATE FUNCTION guard_sales_allocation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE doc_xid xid8; doc_xmin xid; doc_order uuid; line_order uuid;
BEGIN
    IF TG_TABLE_NAME='shipment_allocations' THEN
      SELECT posting_transaction,xmin,order_id INTO doc_xid,doc_xmin,doc_order FROM shipment_postings WHERE id=NEW.shipment_id;
      SELECT order_id INTO line_order FROM sales_order_lines WHERE id=NEW.line_id;
    ELSIF TG_TABLE_NAME='invoice_allocations' THEN
      SELECT posting_transaction,xmin,order_id INTO doc_xid,doc_xmin,doc_order FROM invoices WHERE id=NEW.invoice_id;
      SELECT order_id INTO line_order FROM sales_order_lines WHERE id=NEW.line_id;
    ELSE
      SELECT i.posting_transaction,i.xmin,i.order_id INTO doc_xid,doc_xmin,doc_order FROM invoices i
        JOIN invoice_allocations a ON a.invoice_id=i.id WHERE a.id=NEW.invoice_allocation_id;
      SELECT l.order_id INTO line_order FROM shipment_allocations a JOIN sales_order_lines l ON l.id=a.line_id WHERE a.id=NEW.shipment_allocation_id;
    END IF;
    IF doc_xid IS DISTINCT FROM pg_current_xact_id() OR doc_xmin::text::bigint <> (pg_current_xact_id()::text::numeric % 4294967296)::bigint THEN
      RAISE EXCEPTION 'Cannot append allocations to a posted sales document';
    END IF;
    IF doc_order IS DISTINCT FROM line_order THEN RAISE EXCEPTION 'Allocation belongs to another order'; END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER shipment_allocation_original_transaction BEFORE INSERT ON shipment_allocations FOR EACH ROW EXECUTE FUNCTION guard_sales_allocation();
CREATE TRIGGER invoice_allocation_original_transaction BEFORE INSERT ON invoice_allocations FOR EACH ROW EXECUTE FUNCTION guard_sales_allocation();
CREATE TRIGGER invoice_shipment_original_transaction BEFORE INSERT ON invoice_shipment_allocations FOR EACH ROW EXECUTE FUNCTION guard_sales_allocation();
CREATE TRIGGER immutable_shipment_allocations BEFORE UPDATE OR DELETE ON shipment_allocations FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_invoice_allocations BEFORE UPDATE OR DELETE ON invoice_allocations FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_invoice_shipment_allocations BEFORE UPDATE OR DELETE ON invoice_shipment_allocations FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_shipment_allocations_truncate BEFORE TRUNCATE ON shipment_allocations FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_invoice_allocations_truncate BEFORE TRUNCATE ON invoice_allocations FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_invoice_shipment_allocations_truncate BEFORE TRUNCATE ON invoice_shipment_allocations FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();

CREATE FUNCTION sales_line_progress(target uuid) RETURNS TABLE(line_id uuid,shipped numeric,invoiced numeric,invoiced_value numeric) LANGUAGE sql STABLE AS $$
 SELECT l.id,
 COALESCE((SELECT sum(a.quantity) FROM shipment_allocations a JOIN shipment_postings s ON s.id=a.shipment_id WHERE a.line_id=l.id AND NOT s.reversed),0),
 COALESCE((SELECT sum(a.quantity) FROM invoice_allocations a JOIN invoices i ON i.id=a.invoice_id WHERE a.line_id=l.id AND i.status<>'void'),0),
 COALESCE((SELECT sum(a.total) FROM invoice_allocations a JOIN invoices i ON i.id=a.invoice_id WHERE a.line_id=l.id AND i.status<>'void'),0)
 FROM sales_order_lines l WHERE l.order_id=target
$$;
CREATE FUNCTION refresh_sales_order(target uuid) RETURNS void LANGUAGE sql AS $$
 UPDATE sales_orders o SET status=CASE
   WHEN NOT EXISTS(SELECT 1 FROM sales_order_lines l JOIN sales_line_progress(target) p ON p.line_id=l.id WHERE l.order_id=target AND p.invoiced<l.quantity) THEN 'invoiced'
   WHEN EXISTS(SELECT 1 FROM sales_order_lines WHERE order_id=target AND product_kind='stock') AND NOT EXISTS(SELECT 1 FROM sales_order_lines l JOIN sales_line_progress(target) p ON p.line_id=l.id WHERE l.order_id=target AND l.product_kind='stock' AND p.shipped<l.quantity) THEN 'shipped'
   ELSE 'confirmed' END, version=version+1,
   shipped_at=(SELECT max(created_at) FROM shipment_postings WHERE order_id=target AND NOT reversed),
   invoiced_at=(SELECT max(created_at) FROM invoices WHERE order_id=target AND status<>'void')
 WHERE o.id=target
$$;
CREATE FUNCTION sales_invoice_document(target uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('id',i.id,'number',i.number,'order_id',i.order_id,'customer_id',i.customer_id,
 'customer_name',c.name,'status',i.status,'total',i.total::text,'paid',i.paid::text,
 'outstanding',(CASE WHEN i.status='void' THEN 0 ELSE i.total-i.paid END)::text,'created_at',i.created_at,
 'items',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',a.id,'line_id',l.id,'product_id',l.product_id,
 'sku',l.product_sku,'name',l.product_name,'product_name',l.product_name,'product_kind',l.product_kind,
 'quantity',a.quantity::text,'unit_price',l.unit_price::text,'total',a.total::text) ORDER BY l.product_sku)
 FROM invoice_allocations a JOIN sales_order_lines l ON l.id=a.line_id WHERE a.invoice_id=i.id),'[]'::jsonb))
 FROM invoices i JOIN customers c ON c.id=i.customer_id WHERE i.id=target
$$;
CREATE FUNCTION sales_order_document(target uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('id',o.id,'number',o.number,'customer_id',o.customer_id,'customer_name',c.name,
 'status',o.status,'version',o.version,'total',o.total::text,'created_at',o.created_at,
 'paid',COALESCE((SELECT sum(paid) FROM invoices WHERE order_id=o.id AND status<>'void'),0)::text,
 'invoice_id',(SELECT id FROM invoices WHERE order_id=o.id AND status<>'void' ORDER BY created_at DESC,id DESC LIMIT 1),
 'has_stock_items',EXISTS(SELECT 1 FROM sales_order_lines WHERE order_id=o.id AND product_kind='stock'),
 'can_ship',o.status NOT IN ('draft','cancelled') AND EXISTS(SELECT 1 FROM sales_order_lines l JOIN sales_line_progress(o.id) p ON p.line_id=l.id WHERE l.product_kind='stock' AND l.order_id=o.id AND p.shipped<l.quantity),
 'can_invoice',o.status NOT IN ('draft','cancelled') AND EXISTS(SELECT 1 FROM sales_order_lines l JOIN sales_line_progress(o.id) p ON p.line_id=l.id WHERE l.order_id=o.id AND p.invoiced<CASE WHEN l.product_kind='service' THEN l.quantity ELSE p.shipped END),
 'items',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',l.id,'product_id',l.product_id,'sku',l.product_sku,
 'name',l.product_name,'product_kind',l.product_kind,'quantity',l.quantity::text,'unit_price',l.unit_price::text,'total',l.total::text,
 'shipped_quantity',p.shipped::text,'unshipped_quantity',(CASE WHEN l.product_kind='stock' THEN l.quantity-p.shipped ELSE 0 END)::text,
 'invoiced_quantity',p.invoiced::text,'invoiced_total',p.invoiced_value::text,'invoiceable_quantity',(CASE WHEN l.product_kind='service' THEN l.quantity ELSE p.shipped END-p.invoiced)::text,
 'uninvoiced_quantity',(l.quantity-p.invoiced)::text) ORDER BY l.product_sku)
 FROM sales_order_lines l JOIN sales_line_progress(o.id) p ON p.line_id=l.id WHERE l.order_id=o.id),'[]'::jsonb),
 'shipments',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',s.id,'created_at',s.created_at,'reversed',s.reversed,
 'reversal_reason',(SELECT reason FROM posting_reversals WHERE source_type='shipment' AND source_id=s.id),
 'can_reverse',NOT s.reversed AND NOT EXISTS(SELECT 1 FROM shipment_allocations a JOIN invoice_shipment_allocations x ON x.shipment_allocation_id=a.id JOIN invoice_allocations ia ON ia.id=x.invoice_allocation_id JOIN invoices i ON i.id=ia.invoice_id WHERE a.shipment_id=s.id AND i.status<>'void'),
 'items',(SELECT jsonb_agg(jsonb_build_object('line_id',a.line_id,'sku',l.product_sku,'name',l.product_name,'quantity',a.quantity::text,'value',m.value::text) ORDER BY l.product_sku) FROM shipment_allocations a JOIN sales_order_lines l ON l.id=a.line_id JOIN inventory_movements m ON m.id=a.movement_id WHERE a.shipment_id=s.id)) ORDER BY s.created_at,s.id) FROM shipment_postings s WHERE s.order_id=o.id),'[]'::jsonb),
 'invoices',COALESCE((SELECT jsonb_agg(sales_invoice_document(i.id) ORDER BY i.created_at,i.id) FROM invoices i WHERE i.order_id=o.id),'[]'::jsonb))
 FROM sales_orders o JOIN customers c ON c.id=o.customer_id WHERE o.id=target
$$;

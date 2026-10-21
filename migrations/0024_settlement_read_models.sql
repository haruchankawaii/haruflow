CREATE FUNCTION default_sales_invoice_terms() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 -- Existing rows are untouched. New application postings use now(); explicit
 -- historical imports retain unknown dates unless the operator supplies them.
 IF NEW.invoice_date IS NULL AND NEW.created_at=now() THEN
  SELECT (now() AT TIME ZONE timezone)::date INTO NEW.invoice_date FROM company;
  SELECT NEW.invoice_date+payment_terms_days INTO NEW.due_date FROM customers WHERE id=NEW.customer_id;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER sales_invoice_default_terms BEFORE INSERT ON invoices FOR EACH ROW EXECUTE FUNCTION default_sales_invoice_terms();
CREATE FUNCTION party_document_settlement(target_kind text,target uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 WITH document AS(
 SELECT i.id,i.paid,
  COALESCE((SELECT sum(p.amount) FROM payments p WHERE p.invoice_id=i.id AND NOT EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='payment' AND r.source_id=p.id)),0) cash_total,
  COALESCE((SELECT sum(r.refund+r.settlement_credit) FROM sales_returns r WHERE r.invoice_id=i.id AND NOT EXISTS(SELECT 1 FROM posting_reversals v WHERE v.source_type='sales_return' AND v.source_id=r.id)),0) released,
  COALESCE((SELECT sum(r.refund) FROM sales_returns r WHERE r.invoice_id=i.id AND NOT EXISTS(SELECT 1 FROM posting_reversals v WHERE v.source_type='sales_return' AND v.source_id=r.id)),0) refunded
 FROM invoices i WHERE target_kind='invoice' AND i.id=target
 UNION ALL
 SELECT b.id,b.paid,
  COALESCE((SELECT sum(p.amount) FROM supplier_payments p WHERE p.bill_id=b.id AND NOT EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='supplier_payment' AND r.source_id=p.id)),0),
  COALESCE((SELECT sum(r.refund+r.settlement_credit) FROM supplier_returns r WHERE r.bill_id=b.id AND NOT EXISTS(SELECT 1 FROM posting_reversals v WHERE v.source_type='supplier_return' AND v.source_id=r.id)),0),
  COALESCE((SELECT sum(r.refund) FROM supplier_returns r WHERE r.bill_id=b.id AND NOT EXISTS(SELECT 1 FROM posting_reversals v WHERE v.source_type='supplier_return' AND v.source_id=r.id)),0)
 FROM purchase_bills b WHERE target_kind='bill' AND b.id=target)
 SELECT jsonb_build_object('cash_paid',(cash_total-refunded)::text,
 'allocated_paid',COALESCE((SELECT sum(a.amount) FROM party_allocations a WHERE a.target_type=target_kind AND a.target_id=target AND party_reversal_document('party_allocation',a.id) IS NULL),0)::text,
 'credit_transferred',(released-refunded)::text,'settled_total',paid::text,
 'cash_received_or_paid',(cash_total-refunded)::text,
 'fund_allocations',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',a.id,'fund_id',a.fund_id,'fund_number',f.number,
 'kind',f.kind,'amount',a.amount::text,'created_at',a.created_at,'journal_id',a.journal_id,
 'reversed',party_reversal_document('party_allocation',a.id) IS NOT NULL,'reversal',party_reversal_document('party_allocation',a.id)) ORDER BY a.created_at,a.id)
 FROM party_allocations a JOIN party_funds f ON f.id=a.fund_id WHERE a.target_type=target_kind AND a.target_id=target),'[]'::jsonb)) FROM document
$$;
ALTER FUNCTION sales_invoice_document(uuid) RENAME TO pre_settlement_sales_invoice_document;
CREATE FUNCTION sales_invoice_document(target uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT pre_settlement_sales_invoice_document(target)||party_document_settlement('invoice',target)||jsonb_build_object(
 'invoice_date',CASE WHEN t.id IS NULL THEN i.invoice_date ELSE t.invoice_date END,
 'due_date',CASE WHEN t.id IS NULL THEN i.due_date ELSE t.due_date END,
 'terms_history',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',h.id,'invoice_date',h.invoice_date,'due_date',h.due_date,'reason',h.reason,'created_at',h.created_at) ORDER BY h.created_at,h.id)
 FROM invoice_terms_changes h WHERE h.invoice_id=i.id),'[]'::jsonb))
 FROM invoices i LEFT JOIN LATERAL(SELECT * FROM invoice_terms_changes WHERE invoice_id=i.id ORDER BY created_at DESC,id DESC LIMIT 1)t ON true WHERE i.id=target
$$;

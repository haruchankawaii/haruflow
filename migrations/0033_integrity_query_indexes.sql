-- Child lookups used by deferred ledger checks, settlement reports, and
-- purchasing/stock dependency guards. No existing values or history are changed.
CREATE INDEX journal_lines_journal_idx ON journal_lines(journal_id);
CREATE INDEX payments_invoice_history_idx ON payments(invoice_id,created_at,id);
CREATE INDEX purchase_receipt_lines_product_idx ON purchase_receipt_lines(product_id);
CREATE INDEX sales_order_lines_product_idx ON sales_order_lines(product_id);

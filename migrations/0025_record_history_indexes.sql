-- Stable ordering for the server history pages; totals remain full-scope.
CREATE INDEX customers_history_idx ON customers(created_at DESC,id DESC);
CREATE INDEX suppliers_history_idx ON suppliers(created_at DESC,id DESC);
CREATE INDEX products_history_idx ON products(created_at DESC,id DESC);
CREATE INDEX sales_orders_history_idx ON sales_orders(created_at DESC,id DESC);
CREATE INDEX invoices_history_idx ON invoices(created_at DESC,id DESC);
CREATE INDEX inventory_history_idx ON inventory_movements(created_at DESC,id DESC);
CREATE INDEX journal_history_idx ON journal_entries(created_at DESC,id DESC);
CREATE INDEX audit_history_idx ON audit_events(created_at DESC,id DESC);

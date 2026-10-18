-- Existing products and historical order lines remain stock items.
ALTER TABLE products ADD COLUMN kind text NOT NULL DEFAULT 'stock'
    CHECK (kind IN ('stock','service'));
ALTER TABLE products ADD CONSTRAINT service_products_have_no_inventory
    CHECK (kind <> 'service' OR (quantity=0 AND average_cost=0 AND inventory_value=0));

-- Fulfilment uses the type agreed on the order, independently of catalog edits.
ALTER TABLE sales_order_lines ADD COLUMN product_kind text NOT NULL DEFAULT 'stock'
    CHECK (product_kind IN ('stock','service'));
CREATE TRIGGER immutable_order_product_kind BEFORE UPDATE OF product_kind ON sales_order_lines
    FOR EACH ROW WHEN (OLD.product_kind IS DISTINCT FROM NEW.product_kind)
    EXECUTE FUNCTION immutable_posting();

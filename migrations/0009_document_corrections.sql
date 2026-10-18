ALTER TABLE posting_reversals DROP CONSTRAINT posting_reversals_source_type_check;
ALTER TABLE posting_reversals ADD CONSTRAINT posting_reversals_source_type_check
    CHECK (source_type IN ('payment','invoice','shipment','opening','manual','receipt'));

-- Transaction timestamps can precede time spent waiting for a product lock.
-- New movements receive an insertion sequence while the product is locked.
-- Historical movements retain a deterministic created_at/id order.
CREATE SEQUENCE inventory_movement_sequence;
ALTER TABLE inventory_movements ADD COLUMN movement_seq bigint;
ALTER TABLE inventory_movements DISABLE TRIGGER immutable_inventory;
WITH ordered AS (
    SELECT id,row_number() OVER (ORDER BY created_at,id) sequence FROM inventory_movements
)
UPDATE inventory_movements m SET movement_seq=o.sequence FROM ordered o WHERE o.id=m.id;
ALTER TABLE inventory_movements ENABLE TRIGGER immutable_inventory;
SELECT setval('inventory_movement_sequence',COALESCE(max(movement_seq),0)+1,false)
    FROM inventory_movements;
ALTER TABLE inventory_movements ALTER COLUMN movement_seq SET DEFAULT nextval('inventory_movement_sequence');
ALTER SEQUENCE inventory_movement_sequence OWNED BY inventory_movements.movement_seq;
ALTER TABLE inventory_movements ALTER COLUMN movement_seq SET NOT NULL;
ALTER TABLE inventory_movements ADD CONSTRAINT inventory_movement_sequence_unique UNIQUE (movement_seq);
CREATE INDEX inventory_product_sequence_idx ON inventory_movements(product_id,movement_seq);

-- New receipts preserve the exact pre-receipt average, including rounding left
-- by earlier shipments. Receipts without this snapshot cannot be reversed safely.
ALTER TABLE inventory_movements ADD COLUMN before_quantity numeric(18,3);
ALTER TABLE inventory_movements ADD COLUMN before_average_cost numeric(18,4);
ALTER TABLE inventory_movements ADD COLUMN before_inventory_value numeric(18,2);
ALTER TABLE inventory_movements ADD CONSTRAINT receipt_snapshot_check CHECK (
    (before_quantity IS NULL AND before_average_cost IS NULL AND before_inventory_value IS NULL) OR
    (kind='receipt' AND before_quantity IS NOT NULL AND before_average_cost IS NOT NULL
        AND before_inventory_value IS NOT NULL AND before_quantity>=0 AND before_average_cost>=0
        AND before_inventory_value>=0 AND (before_quantity>0 OR before_inventory_value=0)));

ALTER TABLE inventory_movements DROP CONSTRAINT inventory_movements_kind_check;
ALTER TABLE inventory_movements DROP CONSTRAINT inventory_movements_direction_check;
ALTER TABLE inventory_movements ADD CONSTRAINT inventory_movements_kind_check
    CHECK (kind IN ('receipt','shipment','adjustment','shipment_reversal','opening','opening_reversal','receipt_reversal'));
ALTER TABLE inventory_movements ADD CONSTRAINT inventory_movements_direction_check CHECK (
    (kind IN ('receipt','shipment_reversal','opening') AND quantity>0) OR
    (kind IN ('shipment','opening_reversal','receipt_reversal') AND quantity<0) OR
    (kind='adjustment' AND quantity<>0));
ALTER TABLE inventory_movements ADD CONSTRAINT receipt_reversal_reference_check
    CHECK (kind<>'receipt_reversal' OR reverses_id IS NOT NULL);

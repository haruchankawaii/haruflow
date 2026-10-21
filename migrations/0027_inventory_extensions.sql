CREATE TABLE warehouses(
 id uuid PRIMARY KEY,code text NOT NULL UNIQUE CHECK(code~'^[A-Z0-9][A-Z0-9_-]{0,31}$'),
 name text NOT NULL CHECK(char_length(name) BETWEEN 1 AND 120),active boolean NOT NULL DEFAULT true,
 is_default boolean NOT NULL DEFAULT false,version bigint NOT NULL DEFAULT 1 CHECK(version>0),created_at timestamptz NOT NULL DEFAULT now());
CREATE UNIQUE INDEX warehouse_single_default ON warehouses(is_default) WHERE is_default;
INSERT INTO warehouses(id,code,name,is_default) VALUES('00000000-0000-0000-0000-000000000001','UTAMA','Gudang utama',true);
CREATE TABLE product_inventory_settings(
 product_id uuid PRIMARY KEY REFERENCES products(id) ON DELETE CASCADE,
 tracking text NOT NULL DEFAULT 'none' CHECK(tracking IN ('none','lot','serial')),expiry_required boolean NOT NULL DEFAULT false,
 version bigint NOT NULL DEFAULT 1 CHECK(version>0));
CREATE TABLE product_units(
 id uuid PRIMARY KEY,product_id uuid NOT NULL REFERENCES products(id),name text NOT NULL CHECK(char_length(name) BETWEEN 1 AND 32),
 numerator numeric(18,0) NOT NULL CHECK(numerator>0 AND numerator<=1000000000),denominator numeric(18,0) NOT NULL CHECK(denominator>0 AND denominator<=1000000000),
 created_at timestamptz NOT NULL DEFAULT now(),UNIQUE(product_id,name));
CREATE TABLE stock_lots(
 id uuid PRIMARY KEY,product_id uuid NOT NULL REFERENCES products(id),code text NOT NULL CHECK(char_length(code) BETWEEN 1 AND 100 AND code!~'[[:cntrl:]]'),
 kind text NOT NULL CHECK(kind IN ('lot','serial')),expires_on date,created_at timestamptz NOT NULL DEFAULT now(),UNIQUE(product_id,code),UNIQUE(id,product_id));
CREATE TABLE stock_buckets(
 id uuid PRIMARY KEY,product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,warehouse_id uuid NOT NULL REFERENCES warehouses(id),lot_id uuid,
 quantity numeric(18,3) NOT NULL DEFAULT 0 CHECK(quantity>=0),inventory_value numeric(18,2) NOT NULL DEFAULT 0 CHECK(inventory_value>=0),
 version bigint NOT NULL DEFAULT 1 CHECK(version>0),FOREIGN KEY(lot_id,product_id) REFERENCES stock_lots(id,product_id));
CREATE UNIQUE INDEX stock_bucket_identity ON stock_buckets(product_id,warehouse_id,COALESCE(lot_id,'00000000-0000-0000-0000-000000000000'::uuid));
INSERT INTO stock_buckets(id,product_id,warehouse_id,quantity,inventory_value)
 SELECT gen_random_uuid(),id,'00000000-0000-0000-0000-000000000001',quantity,inventory_value FROM products WHERE kind='stock';
CREATE TABLE warehouse_transfers(
 id uuid PRIMARY KEY,number text NOT NULL UNIQUE,from_warehouse_id uuid NOT NULL REFERENCES warehouses(id),to_warehouse_id uuid NOT NULL REFERENCES warehouses(id),
 reference text,reason text NOT NULL,created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now(),CHECK(from_warehouse_id<>to_warehouse_id));
CREATE SEQUENCE warehouse_transfer_number;
CREATE TABLE stock_location_events(
 id uuid PRIMARY KEY,event_seq bigserial UNIQUE,product_id uuid NOT NULL REFERENCES products(id),warehouse_id uuid NOT NULL REFERENCES warehouses(id),lot_id uuid,
 kind text NOT NULL CHECK(kind IN ('migration','movement','transfer','assignment','valuation')),
 quantity numeric(18,3) NOT NULL,value numeric(18,2) NOT NULL DEFAULT 0,
 movement_id uuid REFERENCES inventory_movements(id),transfer_id uuid REFERENCES warehouse_transfers(id),
 created_by uuid REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now(),FOREIGN KEY(lot_id,product_id) REFERENCES stock_lots(id,product_id),
 CHECK(quantity<>0 OR value<>0));
CREATE INDEX stock_location_events_product_seq ON stock_location_events(product_id,event_seq);
INSERT INTO stock_location_events(id,product_id,warehouse_id,kind,quantity,value)
 SELECT gen_random_uuid(),product_id,warehouse_id,'migration',quantity,inventory_value FROM stock_buckets WHERE quantity<>0 OR inventory_value<>0;
CREATE TABLE stock_reservations(
 id uuid PRIMARY KEY,bucket_id uuid NOT NULL REFERENCES stock_buckets(id),quantity numeric(18,3) NOT NULL CHECK(quantity>0),
 order_id uuid REFERENCES sales_orders(id),reference text,expires_at timestamptz,created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now(),
 version bigint NOT NULL DEFAULT 1 CHECK(version>0));
CREATE TABLE stock_reservation_events(
 id uuid PRIMARY KEY,reservation_id uuid NOT NULL REFERENCES stock_reservations(id),
 kind text NOT NULL CHECK(kind IN ('consume','release','restore')),quantity numeric(18,3) NOT NULL CHECK(quantity<>0),
 movement_id uuid REFERENCES inventory_movements(id),reason text,created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now(),
 CHECK((kind='restore' AND quantity<0) OR (kind<>'restore' AND quantity>0)));
CREATE INDEX stock_reservation_bucket_idx ON stock_reservations(bucket_id,expires_at);
CREATE FUNCTION stock_reservation_remaining(target uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
 SELECT r.quantity-COALESCE((SELECT sum(e.quantity) FROM stock_reservation_events e WHERE e.reservation_id=r.id),0) FROM stock_reservations r WHERE r.id=target
$$;
CREATE FUNCTION stock_bucket_reserved(target uuid,except_order uuid DEFAULT NULL) RETURNS numeric LANGUAGE sql STABLE AS $$
 SELECT COALESCE(sum(stock_reservation_remaining(r.id)),0) FROM stock_reservations r WHERE r.bucket_id=target
 AND (r.expires_at IS NULL OR r.expires_at>now()) AND (except_order IS NULL OR r.order_id IS DISTINCT FROM except_order)
$$;
CREATE FUNCTION stock_rebalance(product uuid,movement uuid DEFAULT NULL,transfer uuid DEFAULT NULL,actor uuid DEFAULT NULL) RETURNS void LANGUAGE plpgsql AS $$
DECLARE total_quantity numeric; total_value numeric; remainder numeric; row_data record; target_value numeric;
BEGIN
 SELECT quantity,inventory_value INTO total_quantity,total_value FROM products WHERE id=product;
 IF total_quantity<>(SELECT COALESCE(sum(quantity),0) FROM stock_buckets WHERE product_id=product) THEN
  RAISE EXCEPTION 'Jumlah stok per gudang tidak cocok dengan stok produk' USING ERRCODE='23514';
 END IF;
 remainder:=total_value;
 FOR row_data IN SELECT b.*,row_number() OVER(ORDER BY b.warehouse_id,b.lot_id NULLS FIRST,b.id) position,
  count(*) OVER() row_count FROM stock_buckets b WHERE b.product_id=product AND b.quantity>0 ORDER BY b.warehouse_id,b.lot_id NULLS FIRST,b.id LOOP
  target_value:=CASE WHEN row_data.position=row_data.row_count THEN remainder ELSE LEAST(remainder,round(total_value*row_data.quantity/total_quantity,2)) END;
  IF target_value<>row_data.inventory_value THEN
   INSERT INTO stock_location_events(id,product_id,warehouse_id,lot_id,kind,quantity,value,movement_id,transfer_id,created_by)
    VALUES(gen_random_uuid(),product,row_data.warehouse_id,row_data.lot_id,'valuation',0,target_value-row_data.inventory_value,movement,transfer,actor);
  END IF;
  UPDATE stock_buckets SET inventory_value=target_value,version=version+CASE WHEN inventory_value<>target_value THEN 1 ELSE 0 END WHERE id=row_data.id;
  remainder:=remainder-target_value;
 END LOOP;
 FOR row_data IN SELECT * FROM stock_buckets WHERE product_id=product AND quantity=0 AND inventory_value<>0 LOOP
  INSERT INTO stock_location_events(id,product_id,warehouse_id,lot_id,kind,quantity,value,movement_id,transfer_id,created_by)
   VALUES(gen_random_uuid(),product,row_data.warehouse_id,row_data.lot_id,'valuation',0,-row_data.inventory_value,movement,transfer,actor);
  UPDATE stock_buckets SET inventory_value=0,version=version+1 WHERE id=row_data.id;
 END LOOP;
END $$;
CREATE FUNCTION verify_stock_locations() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE target uuid; qty numeric; carrying numeric; global_qty numeric; global_value numeric;
BEGIN
 IF TG_TABLE_NAME='products' THEN target:=NEW.id;ELSE target:=NEW.product_id;END IF;
 SELECT quantity,inventory_value INTO global_qty,global_value FROM products WHERE id=target;
 SELECT COALESCE(sum(quantity),0),COALESCE(sum(inventory_value),0) INTO qty,carrying FROM stock_buckets WHERE product_id=target;
 IF qty<>global_qty OR carrying<>global_value THEN RAISE EXCEPTION 'Saldo lokasi harus sama dengan stok global produk' USING ERRCODE='23514'; END IF;
 IF EXISTS(SELECT 1 FROM stock_buckets b JOIN stock_lots l ON l.id=b.lot_id WHERE b.product_id=target AND l.kind='serial' AND b.quantity NOT IN (0,1)) OR
  EXISTS(SELECT 1 FROM stock_buckets b JOIN stock_lots l ON l.id=b.lot_id WHERE b.product_id=target AND l.kind='serial' GROUP BY l.id HAVING sum(b.quantity)>1) THEN
  RAISE EXCEPTION 'Satu nomor seri hanya boleh memiliki satu unit stok' USING ERRCODE='23514';
 END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER product_stock_location_balance AFTER INSERT OR UPDATE ON products DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION verify_stock_locations();
CREATE CONSTRAINT TRIGGER bucket_stock_location_balance AFTER INSERT OR UPDATE ON stock_buckets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION verify_stock_locations();
CREATE FUNCTION stock_apply_bucket(product uuid,warehouse uuid,lot uuid,delta numeric,movement inventory_movements) RETURNS void LANGUAGE plpgsql AS $$
DECLARE bucket uuid; available numeric; held numeric; matching uuid; needed numeric; reservation record; consume numeric; mode text; lot_kind text;
BEGIN
 IF NOT EXISTS(SELECT 1 FROM warehouses WHERE id=warehouse AND active) THEN RAISE EXCEPTION 'Gudang tidak aktif atau tidak ditemukan' USING ERRCODE='23514'; END IF;
 SELECT tracking INTO mode FROM product_inventory_settings WHERE product_id=product; mode:=COALESCE(mode,'none');
 IF lot IS NOT NULL THEN
  SELECT kind INTO lot_kind FROM stock_lots WHERE id=lot AND product_id=product;
  IF lot_kind IS NULL OR (mode<>'none' AND mode<>lot_kind) THEN RAISE EXCEPTION 'Lot/nomor seri tidak cocok dengan produk' USING ERRCODE='23514'; END IF;
  IF lot_kind='serial' AND abs(delta)<>1 THEN RAISE EXCEPTION 'Setiap nomor seri harus tepat satu unit' USING ERRCODE='23514'; END IF;
 ELSIF mode<>'none' THEN RAISE EXCEPTION 'Produk terlacak memerlukan lot atau nomor seri yang diketahui' USING ERRCODE='23514'; END IF;
 IF delta>0 AND EXISTS(SELECT 1 FROM product_inventory_settings s WHERE s.product_id=product AND s.expiry_required)
  AND (lot IS NULL OR NOT EXISTS(SELECT 1 FROM stock_lots WHERE id=lot AND expires_on IS NOT NULL)) THEN
  RAISE EXCEPTION 'Tanggal kedaluwarsa lot wajib diisi' USING ERRCODE='23514';
 END IF;
 IF delta<0 AND movement.kind='shipment' AND EXISTS(SELECT 1 FROM stock_lots l CROSS JOIN company c WHERE l.id=lot AND l.expires_on<(now() AT TIME ZONE c.timezone)::date) THEN
  RAISE EXCEPTION 'Lot kedaluwarsa tidak dapat dikirim ke pelanggan' USING ERRCODE='23514';
 END IF;
 INSERT INTO stock_buckets(id,product_id,warehouse_id,lot_id) VALUES(gen_random_uuid(),product,warehouse,lot) ON CONFLICT DO NOTHING;
 SELECT id,quantity INTO bucket,available FROM stock_buckets WHERE product_id=product AND warehouse_id=warehouse AND lot_id IS NOT DISTINCT FROM lot FOR UPDATE;
 IF delta<0 THEN
  matching:=CASE WHEN movement.kind='shipment' THEN movement.order_id ELSE NULL END;
  held:=stock_bucket_reserved(bucket,matching);
  IF available-held<abs(delta) THEN RAISE EXCEPTION 'Stok tersedia di gudang/lot tidak cukup atau telah direservasi' USING ERRCODE='23514'; END IF;
  IF matching IS NOT NULL THEN
   needed:=abs(delta);
   FOR reservation IN SELECT r.id,stock_reservation_remaining(r.id) remaining FROM stock_reservations r WHERE r.bucket_id=bucket AND r.order_id=matching AND (r.expires_at IS NULL OR r.expires_at>now()) ORDER BY r.created_at,r.id FOR UPDATE LOOP
    consume:=LEAST(needed,reservation.remaining);
    IF consume>0 THEN INSERT INTO stock_reservation_events(id,reservation_id,kind,quantity,movement_id,created_by) VALUES(gen_random_uuid(),reservation.id,'consume',consume,movement.id,movement.created_by);
     UPDATE stock_reservations SET version=version+1 WHERE id=reservation.id;
    END IF;
    needed:=needed-consume;EXIT WHEN needed=0;
   END LOOP;
  END IF;
 END IF;
 UPDATE stock_buckets SET quantity=quantity+delta,version=version+1 WHERE id=bucket;
 INSERT INTO stock_location_events(id,product_id,warehouse_id,lot_id,kind,quantity,value,movement_id,created_by)
  VALUES(gen_random_uuid(),product,warehouse,lot,'movement',delta,0,movement.id,movement.created_by);
END $$;
CREATE FUNCTION apply_inventory_locations() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE context jsonb; selected jsonb; warehouse uuid; lot uuid; total numeric:=0; qty numeric; remaining numeric; bucket record; tracking text; original_seq bigint; event record;
BEGIN
 PERFORM pg_advisory_xact_lock(8414077);
 context:=COALESCE(NULLIF(current_setting('haruflow.stock_context',true),''),'{}')::jsonb;
 -- Consume context exactly once; every physical document line sets its own.
 PERFORM set_config('haruflow.stock_context','',true);
 SELECT id INTO warehouse FROM warehouses WHERE is_default;
 warehouse:=COALESCE((context->>'warehouse_id')::uuid,warehouse);lot:=(context->>'lot_id')::uuid;
 IF NEW.reverses_id IS NOT NULL AND EXISTS(SELECT 1 FROM stock_location_events WHERE movement_id=NEW.reverses_id AND kind='movement') THEN
  SELECT max(event_seq) INTO original_seq FROM stock_location_events WHERE movement_id=NEW.reverses_id;
  -- A later movement and its exact reversal cancel as a pair. Transfers and lot
  -- assignments remain dependencies because they have no inverse movement pair.
  -- Shipment reversal is a new receipt at the original cost/location, which can
  -- safely coexist with later movements. Serial uniqueness and global totals
  -- still apply. Other reversals require the original snapshot dependencies.
  IF NEW.kind<>'shipment_reversal' AND EXISTS(SELECT 1 FROM stock_location_events e LEFT JOIN inventory_movements m ON m.id=e.movement_id
    WHERE e.product_id=NEW.product_id AND e.event_seq>original_seq AND NOT COALESCE(
      (m.reverses_id IS NULL AND EXISTS(SELECT 1 FROM inventory_movements r WHERE r.reverses_id=m.id)) OR
      (m.reverses_id IS NOT NULL AND EXISTS(SELECT 1 FROM stock_location_events o WHERE o.movement_id=m.reverses_id AND o.event_seq>original_seq)),false))
  THEN RAISE EXCEPTION 'Ada mutasi lokasi setelah sumber; pembalikan stok historis ditolak' USING ERRCODE='23514'; END IF;
  FOR event IN SELECT warehouse_id,lot_id,-quantity delta FROM stock_location_events WHERE movement_id=NEW.reverses_id AND kind='movement' ORDER BY event_seq LOOP
   PERFORM stock_apply_bucket(NEW.product_id,event.warehouse_id,event.lot_id,event.delta,NEW);total:=total+event.delta;
  END LOOP;
  FOR event IN SELECT * FROM stock_reservation_events WHERE movement_id=NEW.reverses_id AND kind='consume' LOOP
   INSERT INTO stock_reservation_events(id,reservation_id,kind,quantity,movement_id,created_by) VALUES(gen_random_uuid(),event.reservation_id,'restore',-event.quantity,NEW.id,NEW.created_by);
   UPDATE stock_reservations SET version=version+1 WHERE id=event.reservation_id;
  END LOOP;
 ELSIF jsonb_array_length(COALESCE(context->'allocations','[]'::jsonb))>0 THEN
  FOR selected IN SELECT * FROM jsonb_array_elements(context->'allocations') LOOP
   qty:=(selected->>'quantity')::numeric;
   IF qty<=0 OR qty<>round(qty,3) THEN RAISE EXCEPTION 'Jumlah alokasi lokasi tidak valid' USING ERRCODE='23514'; END IF;
   qty:=CASE WHEN NEW.quantity<0 THEN -qty ELSE qty END;
   PERFORM stock_apply_bucket(NEW.product_id,(selected->>'warehouse_id')::uuid,(selected->>'lot_id')::uuid,qty,NEW);total:=total+qty;
  END LOOP;
 ELSE
  SELECT s.tracking INTO tracking FROM product_inventory_settings s WHERE s.product_id=NEW.product_id;
  IF NEW.quantity<0 AND lot IS NULL AND COALESCE(tracking,'none')<>'none' THEN
   remaining:=-NEW.quantity;
   FOR bucket IN SELECT b.*,l.expires_on FROM stock_buckets b JOIN stock_lots l ON l.id=b.lot_id WHERE b.product_id=NEW.product_id AND b.warehouse_id=warehouse AND b.quantity>0
    AND (NEW.kind<>'shipment' OR l.expires_on IS NULL OR l.expires_on>=(SELECT (now() AT TIME ZONE timezone)::date FROM company))
    ORDER BY l.expires_on NULLS LAST,l.created_at,l.id FOR UPDATE OF b LOOP
    qty:=LEAST(remaining,GREATEST(bucket.quantity-stock_bucket_reserved(bucket.id,CASE WHEN NEW.kind='shipment' THEN NEW.order_id ELSE NULL END),0));
    IF qty>0 THEN PERFORM stock_apply_bucket(NEW.product_id,warehouse,bucket.lot_id,-qty,NEW);remaining:=remaining-qty;END IF;
    EXIT WHEN remaining=0;
   END LOOP;
   IF remaining<>0 THEN RAISE EXCEPTION 'Lot tersedia tidak cukup di gudang pilihan' USING ERRCODE='23514'; END IF;
   total:=NEW.quantity;
  ELSE PERFORM stock_apply_bucket(NEW.product_id,warehouse,lot,NEW.quantity,NEW);total:=NEW.quantity;END IF;
 END IF;
 IF total<>NEW.quantity THEN RAISE EXCEPTION 'Jumlah alokasi lokasi harus sama dengan mutasi stok' USING ERRCODE='23514'; END IF;
 PERFORM stock_rebalance(NEW.product_id,NEW.id,NULL,NEW.created_by);
 RETURN NULL;
END $$;
CREATE TRIGGER inventory_location_posting AFTER INSERT ON inventory_movements FOR EACH ROW EXECUTE FUNCTION apply_inventory_locations();
CREATE FUNCTION stock_reservation_version_only() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF NEW.version<>OLD.version+1 OR (to_jsonb(NEW)-'version') IS DISTINCT FROM (to_jsonb(OLD)-'version') THEN RAISE EXCEPTION 'Riwayat reservasi tidak dapat diubah';END IF;RETURN NEW;
END $$;
CREATE TRIGGER stock_reservation_version BEFORE UPDATE ON stock_reservations FOR EACH ROW EXECUTE FUNCTION stock_reservation_version_only();
CREATE TRIGGER stock_reservation_no_delete BEFORE DELETE ON stock_reservations FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE FUNCTION protect_product_unit_conversion() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF NEW.unit IS DISTINCT FROM OLD.unit AND (EXISTS(SELECT 1 FROM product_units WHERE product_id=OLD.id) OR EXISTS(SELECT 1 FROM product_inventory_settings WHERE product_id=OLD.id AND tracking<>'none')) THEN
  RAISE EXCEPTION 'Satuan dasar tidak dapat diganti setelah konversi atau pelacakan diaktifkan' USING ERRCODE='23514';END IF;RETURN NEW;
END $$;
CREATE TRIGGER product_unit_conversion_guard BEFORE UPDATE ON products FOR EACH ROW EXECUTE FUNCTION protect_product_unit_conversion();
DO $$ DECLARE table_name text;BEGIN
 FOREACH table_name IN ARRAY ARRAY['stock_lots','product_units','warehouse_transfers','stock_location_events','stock_reservation_events'] LOOP
  EXECUTE format('CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION immutable_posting()',table_name||'_immutable',table_name);
  EXECUTE format('CREATE TRIGGER %I BEFORE TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting()',table_name||'_no_truncate',table_name);
 END LOOP;
END $$;

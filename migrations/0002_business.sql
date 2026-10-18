CREATE TABLE customers (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    email text,
    phone text,
    address text,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE products (
    id uuid PRIMARY KEY,
    sku text NOT NULL UNIQUE CHECK (length(sku) BETWEEN 1 AND 64),
    name text NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    unit text NOT NULL DEFAULT 'pcs',
    price numeric(18,2) NOT NULL CHECK (price > 0),
    quantity numeric(18,3) NOT NULL DEFAULT 0 CHECK (quantity >= 0 AND quantity <= 1000000000),
    average_cost numeric(18,4) NOT NULL DEFAULT 0 CHECK (average_cost >= 0),
    inventory_value numeric(18,2) NOT NULL DEFAULT 0 CHECK (inventory_value >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (quantity > 0 OR inventory_value = 0)
);

CREATE SEQUENCE sales_order_number;
CREATE SEQUENCE invoice_number;

CREATE TABLE sales_orders (
    id uuid PRIMARY KEY,
    number text NOT NULL UNIQUE,
    customer_id uuid NOT NULL REFERENCES customers(id),
    status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','confirmed','shipped','invoiced')),
    total numeric(18,2) NOT NULL CHECK (total > 0),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    confirmed_at timestamptz,
    shipped_at timestamptz,
    invoiced_at timestamptz
);

CREATE TABLE sales_order_lines (
    id uuid PRIMARY KEY,
    order_id uuid NOT NULL REFERENCES sales_orders(id),
    product_id uuid NOT NULL REFERENCES products(id),
    product_sku text NOT NULL,
    product_name text NOT NULL,
    quantity numeric(18,3) NOT NULL CHECK (quantity > 0),
    unit_price numeric(18,2) NOT NULL CHECK (unit_price > 0),
    total numeric(18,2) NOT NULL CHECK (total > 0),
    UNIQUE (order_id, product_id)
);

CREATE TABLE invoices (
    id uuid PRIMARY KEY,
    number text NOT NULL UNIQUE,
    order_id uuid NOT NULL UNIQUE REFERENCES sales_orders(id),
    customer_id uuid NOT NULL REFERENCES customers(id),
    total numeric(18,2) NOT NULL CHECK (total > 0),
    paid numeric(18,2) NOT NULL DEFAULT 0 CHECK (paid >= 0 AND paid <= total),
    status text NOT NULL DEFAULT 'unpaid' CHECK (status IN ('unpaid','partial','paid')),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE payments (
    id uuid PRIMARY KEY,
    invoice_id uuid NOT NULL REFERENCES invoices(id),
    amount numeric(18,2) NOT NULL CHECK (amount > 0),
    reference text,
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE inventory_movements (
    id uuid PRIMARY KEY,
    product_id uuid NOT NULL REFERENCES products(id),
    kind text NOT NULL CHECK (kind IN ('receipt','shipment')),
    quantity numeric(18,3) NOT NULL CHECK (quantity <> 0),
    unit_cost numeric(18,4) NOT NULL CHECK (unit_cost >= 0),
    value numeric(18,2) NOT NULL CHECK (value >= 0),
    reference text,
    order_id uuid REFERENCES sales_orders(id),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((kind='receipt' AND quantity>0) OR (kind='shipment' AND quantity<0))
);

CREATE TABLE accounts (
    code text PRIMARY KEY,
    name text NOT NULL,
    kind text NOT NULL CHECK (kind IN ('asset','equity','revenue','expense'))
);
INSERT INTO accounts (code,name,kind) VALUES
('1100','Cash','asset'), ('1200','Accounts receivable','asset'),
('1300','Inventory','asset'), ('3000','Opening equity','equity'),
('4000','Sales revenue','revenue'), ('5000','Cost of goods sold','expense');

CREATE TABLE journal_entries (
    id uuid PRIMARY KEY,
    description text NOT NULL,
    source_type text NOT NULL CHECK (source_type IN ('receipt','shipment','invoice','payment')),
    source_id uuid NOT NULL,
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    posting_transaction xid8 NOT NULL DEFAULT pg_current_xact_id(),
    UNIQUE (source_type,source_id)
);
CREATE TABLE journal_lines (
    id uuid PRIMARY KEY,
    journal_id uuid NOT NULL REFERENCES journal_entries(id),
    account_code text NOT NULL REFERENCES accounts(code),
    debit numeric(18,2) NOT NULL DEFAULT 0,
    credit numeric(18,2) NOT NULL DEFAULT 0,
    CHECK ((debit>0 AND credit=0) OR (credit>0 AND debit=0))
);

-- Deferred checks allow the header and all lines to be inserted in one transaction.
CREATE FUNCTION check_journal_balance() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE target uuid; line_count bigint; balance numeric;
BEGIN
    IF TG_TABLE_NAME = 'journal_entries' THEN target := NEW.id;
    ELSE target := NEW.journal_id; END IF;
    SELECT count(*), COALESCE(sum(debit-credit),0) INTO line_count,balance
        FROM journal_lines WHERE journal_id=target;
    IF line_count<2 OR balance<>0 THEN
        RAISE EXCEPTION 'Journal % must contain at least two balanced lines',target;
    END IF;
    RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER journal_header_balance AFTER INSERT ON journal_entries
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION check_journal_balance();
CREATE CONSTRAINT TRIGGER journal_line_balance AFTER INSERT ON journal_lines
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION check_journal_balance();

CREATE FUNCTION immutable_posting() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'Posted ledger records are immutable'; END $$;
CREATE TRIGGER immutable_inventory BEFORE UPDATE OR DELETE ON inventory_movements
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_journal BEFORE UPDATE OR DELETE ON journal_entries
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_journal_lines BEFORE UPDATE OR DELETE ON journal_lines
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE FUNCTION guard_journal_append() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF (SELECT posting_transaction FROM journal_entries WHERE id=NEW.journal_id) <> pg_current_xact_id() THEN
        RAISE EXCEPTION 'Cannot append lines to a posted journal';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER journal_original_transaction BEFORE INSERT ON journal_lines
    FOR EACH ROW EXECUTE FUNCTION guard_journal_append();
CREATE TRIGGER immutable_payments BEFORE UPDATE OR DELETE ON payments
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();

CREATE TABLE idempotency_keys (
    key uuid PRIMARY KEY,
    request_hash text NOT NULL,
    response jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX customers_name_idx ON customers(name);
CREATE INDEX orders_created_idx ON sales_orders(created_at DESC);
CREATE INDEX movements_product_created_idx ON inventory_movements(product_id,created_at DESC);
CREATE INDEX journal_lines_account_idx ON journal_lines(account_code);
CREATE INDEX journal_created_idx ON journal_entries(created_at DESC);

-- Purchase orders record a purchasing commitment only. Goods receipts and
-- supplier liabilities will be introduced in a later stage.
CREATE TABLE suppliers (
    id uuid PRIMARY KEY,
    code text NOT NULL UNIQUE CHECK (char_length(code) BETWEEN 1 AND 64 AND code = upper(btrim(code)) AND code !~ '[[:cntrl:]]'),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 200 AND name = btrim(name) AND name !~ '[[:cntrl:]]'),
    email text CHECK (email IS NULL OR (char_length(email) BETWEEN 1 AND 254 AND email !~ '[[:cntrl:]]')),
    phone text CHECK (phone IS NULL OR (char_length(phone) BETWEEN 1 AND 64 AND phone !~ '[[:cntrl:]]')),
    address text CHECK (address IS NULL OR char_length(address) BETWEEN 1 AND 1000),
    active boolean NOT NULL DEFAULT true,
    version integer NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX suppliers_name_idx ON suppliers(name,id);

CREATE SEQUENCE purchase_order_number;
CREATE TABLE purchase_orders (
    id uuid PRIMARY KEY,
    number text NOT NULL UNIQUE,
    supplier_id uuid NOT NULL REFERENCES suppliers(id),
    supplier_code text NOT NULL,
    supplier_name text NOT NULL,
    status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','confirmed','cancelled')),
    expected_date date,
    notes text CHECK (notes IS NULL OR char_length(notes) BETWEEN 1 AND 2000),
    total numeric(18,2) NOT NULL CHECK (total > 0 AND total <= 1000000000000),
    version integer NOT NULL DEFAULT 1 CHECK (version > 0),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    confirmed_at timestamptz,
    cancelled_at timestamptz,
    cancellation_reason text CHECK (cancellation_reason IS NULL OR char_length(cancellation_reason) BETWEEN 3 AND 1000),
    CHECK ((status='draft' AND confirmed_at IS NULL AND cancelled_at IS NULL AND cancellation_reason IS NULL)
        OR (status='confirmed' AND confirmed_at IS NOT NULL AND cancelled_at IS NULL AND cancellation_reason IS NULL)
        OR (status='cancelled' AND cancelled_at IS NOT NULL AND cancellation_reason IS NOT NULL))
);
CREATE INDEX purchase_orders_created_idx ON purchase_orders(created_at DESC,id);
CREATE INDEX purchase_orders_supplier_idx ON purchase_orders(supplier_id);

CREATE TABLE purchase_order_lines (
    id uuid PRIMARY KEY,
    order_id uuid NOT NULL REFERENCES purchase_orders(id),
    position integer NOT NULL CHECK (position BETWEEN 1 AND 100),
    product_id uuid NOT NULL REFERENCES products(id),
    product_sku text NOT NULL,
    product_name text NOT NULL,
    unit text NOT NULL,
    quantity numeric(18,3) NOT NULL CHECK (quantity > 0 AND quantity <= 1000000000),
    unit_cost numeric(18,4) NOT NULL CHECK (unit_cost > 0 AND unit_cost <= 1000000000),
    total numeric(18,2) NOT NULL CHECK (total > 0 AND total <= 1000000000000),
    UNIQUE (order_id,product_id),
    UNIQUE (order_id,position)
);
CREATE INDEX purchase_order_lines_product_idx ON purchase_order_lines(product_id);

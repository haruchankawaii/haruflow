ALTER TABLE company
    ADD COLUMN address text CHECK (address IS NULL OR (char_length(address) BETWEEN 1 AND 500 AND replace(replace(address, chr(10), ''), chr(13), '') !~ '[[:cntrl:]]')),
    ADD COLUMN email text CHECK (email IS NULL OR (char_length(email) BETWEEN 3 AND 254 AND email = lower(email) AND email !~ '[[:cntrl:]]')),
    ADD COLUMN phone text CHECK (phone IS NULL OR (char_length(phone) BETWEEN 1 AND 40 AND phone !~ '[[:cntrl:]]')),
    ADD COLUMN tax_id text CHECK (tax_id IS NULL OR (char_length(tax_id) BETWEEN 1 AND 64 AND tax_id !~ '[[:cntrl:]]')),
    ADD COLUMN order_prefix text NOT NULL DEFAULT 'SO' CHECK (order_prefix ~ '^[A-Z][A-Z0-9-]{1,9}$'),
    ADD COLUMN invoice_prefix text NOT NULL DEFAULT 'INV' CHECK (invoice_prefix ~ '^[A-Z][A-Z0-9-]{1,9}$'),
    ADD CONSTRAINT company_idr_currency CHECK (currency = 'IDR'),
    ADD CONSTRAINT company_name_no_controls CHECK (name !~ '[[:cntrl:]]');

-- Prefix changes deliberately retain both existing document numbers and their sequences.

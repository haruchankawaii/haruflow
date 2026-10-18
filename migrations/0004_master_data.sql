ALTER TABLE accounts DROP CONSTRAINT accounts_kind_check;
ALTER TABLE accounts ADD CONSTRAINT accounts_kind_check
    CHECK (kind IN ('asset', 'liability', 'equity', 'revenue', 'expense'));
ALTER TABLE accounts ADD CONSTRAINT accounts_code_check
    CHECK (code ~ '^[0-9]{3,10}$');
ALTER TABLE accounts ADD CONSTRAINT accounts_name_check
    CHECK (char_length(name) BETWEEN 1 AND 200);

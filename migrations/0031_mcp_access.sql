-- AI access is opt-in for existing accounts; this does not create ERP users.
CREATE TABLE ai_access_grants (
    user_id uuid PRIMARY KEY REFERENCES users(id),
    enabled boolean NOT NULL DEFAULT false,
    scopes text[] NOT NULL DEFAULT '{}',
    updated_by uuid NOT NULL REFERENCES users(id),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (scopes <@ ARRAY['finance', 'sales', 'inventory']::text[])
);

CREATE TABLE ai_tokens (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id),
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 80),
    token_hash text NOT NULL UNIQUE CHECK (token_hash ~ '^[0-9a-f]{64}$'),
    scopes text[] NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    last_used_at timestamptz,
    revoked_at timestamptz,
    CHECK (cardinality(scopes) BETWEEN 1 AND 3),
    CHECK (scopes <@ ARRAY['finance', 'sales', 'inventory']::text[]),
    CHECK (expires_at > created_at)
);
CREATE INDEX ai_tokens_user_history ON ai_tokens(user_id, created_at DESC);

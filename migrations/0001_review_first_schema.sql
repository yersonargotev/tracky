PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS institutions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    institution_id TEXT REFERENCES institutions(id),
    label TEXT NOT NULL,
    currency TEXT NOT NULL,
    masked_identifier TEXT,
    kind TEXT,
    is_owned INTEGER NOT NULL DEFAULT 0 CHECK (is_owned IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS source_documents (
    id TEXT PRIMARY KEY,
    input_name TEXT NOT NULL,
    content_sha256 TEXT NOT NULL UNIQUE,
    mime_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    institution_id TEXT REFERENCES institutions(id),
    institution_hint TEXT,
    account_id TEXT REFERENCES accounts(id),
    account_label_hint TEXT,
    account_currency_hint TEXT,
    account_masked_identifier_hint TEXT,
    imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    duplicate_of_source_document_id TEXT REFERENCES source_documents(id)
);

CREATE TABLE IF NOT EXISTS import_batches (
    id TEXT PRIMARY KEY,
    source_document_id TEXT NOT NULL REFERENCES source_documents(id),
    started_at TEXT NOT NULL,
    completed_at TEXT,
    status TEXT NOT NULL CHECK (status IN ('completed', 'completed_with_errors', 'failed')),
    candidate_count INTEGER NOT NULL DEFAULT 0 CHECK (candidate_count >= 0),
    error_count INTEGER NOT NULL DEFAULT 0 CHECK (error_count >= 0),
    duplicate_count INTEGER NOT NULL DEFAULT 0 CHECK (duplicate_count >= 0),
    error_details_json TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE IF NOT EXISTS canonical_transactions (
    id TEXT PRIMARY KEY,
    account_id TEXT REFERENCES accounts(id),
    posted_date TEXT NOT NULL,
    description TEXT NOT NULL,
    amount_minor INTEGER NOT NULL,
    currency TEXT NOT NULL,
    balance_minor INTEGER,
    transaction_kind TEXT,
    created_from_candidate_id TEXT UNIQUE REFERENCES candidate_transactions(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS candidate_transactions (
    id TEXT PRIMARY KEY,
    import_batch_id TEXT NOT NULL REFERENCES import_batches(id),
    source_document_id TEXT NOT NULL REFERENCES source_documents(id),
    institution_id TEXT REFERENCES institutions(id),
    institution_hint TEXT,
    account_id TEXT REFERENCES accounts(id),
    account_label_hint TEXT,
    account_currency_hint TEXT,
    account_masked_identifier_hint TEXT,
    posted_date TEXT NOT NULL,
    description TEXT NOT NULL,
    amount_minor INTEGER NOT NULL,
    currency TEXT NOT NULL,
    balance_minor INTEGER,
    direction_hint TEXT CHECK (direction_hint IN ('inflow', 'outflow')),
    semantic_hint TEXT CHECK (semantic_hint IN ('bank_movement', 'card_charge', 'card_payment')),
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    status TEXT NOT NULL CHECK (status IN ('pending_review', 'possible_duplicate', 'accepted', 'rejected')),
    duplicate_status TEXT NOT NULL DEFAULT 'not_checked' CHECK (duplicate_status IN ('not_checked', 'unique', 'possible_duplicate', 'exact_duplicate')),
    fingerprint TEXT,
    validation_warnings_json TEXT NOT NULL DEFAULT '[]',
    canonical_transaction_id TEXT REFERENCES canonical_transactions(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS provenance (
    id TEXT PRIMARY KEY,
    candidate_transaction_id TEXT UNIQUE REFERENCES candidate_transactions(id),
    canonical_transaction_id TEXT REFERENCES canonical_transactions(id),
    source_document_id TEXT NOT NULL REFERENCES source_documents(id),
    import_batch_id TEXT REFERENCES import_batches(id),
    page_number INTEGER,
    row_index INTEGER,
    bbox_x REAL,
    bbox_y REAL,
    bbox_width REAL,
    bbox_height REAL,
    bbox_unit TEXT,
    extractor_name TEXT NOT NULL,
    extractor_version TEXT,
    parser_id TEXT NOT NULL,
    parser_version TEXT NOT NULL,
    evidence_redaction TEXT NOT NULL,
    evidence_text_redacted TEXT NOT NULL,
    raw_storage_policy TEXT NOT NULL CHECK (raw_storage_policy IN ('not_stored', 'local_only_optional', 'redacted_only')),
    raw_evidence_ref TEXT,
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (candidate_transaction_id IS NOT NULL OR canonical_transaction_id IS NOT NULL)
);

CREATE TABLE IF NOT EXISTS transaction_fingerprints (
    id TEXT PRIMARY KEY,
    fingerprint TEXT NOT NULL UNIQUE,
    candidate_transaction_id TEXT UNIQUE REFERENCES candidate_transactions(id),
    canonical_transaction_id TEXT UNIQUE REFERENCES canonical_transactions(id),
    duplicate_status TEXT NOT NULL CHECK (duplicate_status IN ('not_checked', 'unique', 'possible_duplicate', 'exact_duplicate')),
    normalized_account_key TEXT,
    normalized_posted_date TEXT NOT NULL,
    normalized_amount_minor INTEGER NOT NULL,
    normalized_currency TEXT NOT NULL,
    normalized_description TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (
        (candidate_transaction_id IS NOT NULL AND canonical_transaction_id IS NULL)
        OR (candidate_transaction_id IS NULL AND canonical_transaction_id IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS transaction_duplicate_markers (
    id TEXT PRIMARY KEY,
    candidate_transaction_id TEXT NOT NULL REFERENCES candidate_transactions(id),
    matched_candidate_transaction_id TEXT REFERENCES candidate_transactions(id),
    matched_canonical_transaction_id TEXT REFERENCES canonical_transactions(id),
    duplicate_status TEXT NOT NULL CHECK (duplicate_status IN ('not_checked', 'unique', 'possible_duplicate', 'exact_duplicate')),
    reason TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (
        matched_candidate_transaction_id IS NOT NULL
        OR matched_canonical_transaction_id IS NOT NULL
        OR duplicate_status IN ('not_checked', 'unique')
    )
);

CREATE INDEX IF NOT EXISTS idx_accounts_owned_institution_currency ON accounts(is_owned, institution_id, currency);
CREATE INDEX IF NOT EXISTS idx_source_documents_content_sha256 ON source_documents(content_sha256);
CREATE INDEX IF NOT EXISTS idx_import_batches_source_document_id ON import_batches(source_document_id);
CREATE INDEX IF NOT EXISTS idx_candidate_transactions_batch ON candidate_transactions(import_batch_id);
CREATE INDEX IF NOT EXISTS idx_candidate_transactions_source_document ON candidate_transactions(source_document_id);
CREATE INDEX IF NOT EXISTS idx_candidate_transactions_status ON candidate_transactions(status);
CREATE INDEX IF NOT EXISTS idx_candidate_transactions_fingerprint ON candidate_transactions(fingerprint);
CREATE INDEX IF NOT EXISTS idx_provenance_candidate_transaction ON provenance(candidate_transaction_id);
CREATE INDEX IF NOT EXISTS idx_transaction_duplicate_markers_candidate ON transaction_duplicate_markers(candidate_transaction_id);

PRAGMA user_version = 1;

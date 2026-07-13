CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  applied_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS prompt_profiles (
  id TEXT PRIMARY KEY,
  stable_name TEXT NOT NULL,
  collection TEXT,
  pinned INTEGER NOT NULL DEFAULT 0,
  deleted_at TEXT
);

CREATE TABLE IF NOT EXISTS prompt_versions (
  id TEXT PRIMARY KEY,
  profile_id TEXT NOT NULL REFERENCES prompt_profiles(id),
  version INTEGER NOT NULL,
  source_path TEXT,
  source_hash TEXT NOT NULL,
  front_matter_json TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(profile_id, version),
  UNIQUE(profile_id, source_hash)
);

CREATE TABLE IF NOT EXISTS models (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  display_name TEXT NOT NULL,
  family TEXT,
  format TEXT NOT NULL,
  path TEXT NOT NULL UNIQUE,
  size_bytes INTEGER NOT NULL,
  sha256 TEXT,
  compatibility_json TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  last_verified_at TEXT
);

CREATE TABLE IF NOT EXISTS conversations (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  model_id TEXT REFERENCES models(id),
  prompt_version_id TEXT REFERENCES prompt_versions(id),
  generation_settings_json TEXT NOT NULL,
  context_strategy TEXT NOT NULL,
  pinned INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  parent_id TEXT REFERENCES messages(id),
  role TEXT NOT NULL,
  content_json TEXT NOT NULL,
  token_count INTEGER,
  pinned INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS downloads (
  id TEXT PRIMARY KEY,
  catalog_entry_id TEXT,
  url TEXT NOT NULL,
  destination TEXT NOT NULL,
  partial_path TEXT NOT NULL,
  expected_sha256 TEXT NOT NULL,
  total_bytes INTEGER,
  received_bytes INTEGER NOT NULL DEFAULT 0,
  etag TEXT,
  state TEXT NOT NULL,
  error_json TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS benchmarks (
  id TEXT PRIMARY KEY,
  hardware_fingerprint TEXT NOT NULL,
  engine_id TEXT NOT NULL,
  engine_version TEXT NOT NULL,
  model_hash TEXT NOT NULL,
  settings_json TEXT NOT NULL,
  metrics_json TEXT NOT NULL,
  stable INTEGER NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS outputs (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  file_path TEXT NOT NULL UNIQUE,
  thumbnail_path TEXT,
  source_job_id TEXT,
  metadata_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  state TEXT NOT NULL,
  engine_id TEXT,
  device_id TEXT,
  request_json TEXT NOT NULL,
  result_json TEXT,
  error_json TEXT,
  created_at TEXT NOT NULL,
  started_at TEXT,
  finished_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_conversations_updated ON conversations(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id, created_at);
CREATE INDEX IF NOT EXISTS idx_models_kind ON models(kind);
CREATE INDEX IF NOT EXISTS idx_downloads_state ON downloads(state, updated_at);
CREATE INDEX IF NOT EXISTS idx_outputs_kind ON outputs(kind, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_benchmarks_lookup ON benchmarks(hardware_fingerprint, model_hash, engine_id);

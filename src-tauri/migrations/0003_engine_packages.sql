CREATE TABLE engine_packages (
  id TEXT PRIMARY KEY,
  engine_id TEXT NOT NULL,
  version TEXT NOT NULL,
  platform TEXT NOT NULL,
  architecture TEXT NOT NULL,
  route TEXT NOT NULL,
  install_path TEXT NOT NULL UNIQUE,
  archive_sha256 TEXT NOT NULL,
  file_manifest_json TEXT NOT NULL DEFAULT '[]',
  state TEXT NOT NULL CHECK (state IN ('installing', 'ready', 'invalid', 'missing')),
  source_url TEXT,
  error_json TEXT,
  installed_at TEXT,
  verified_at TEXT,
  UNIQUE(engine_id, version, platform, architecture, route)
);

CREATE INDEX idx_engine_packages_state
  ON engine_packages(engine_id, state, route);

ALTER TABLE models ADD COLUMN verification_state TEXT NOT NULL DEFAULT 'metadata_pending'
  CHECK (verification_state IN ('metadata_pending', 'ready', 'invalid', 'missing'));
ALTER TABLE models ADD COLUMN verification_error TEXT;
ALTER TABLE models ADD COLUMN gguf_metadata_json TEXT NOT NULL DEFAULT 'null';
ALTER TABLE models ADD COLUMN modified_at_unix_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE models ADD COLUMN file_identity TEXT;

CREATE INDEX IF NOT EXISTS idx_models_verification_state
  ON models(verification_state, display_name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_models_file_identity
  ON models(file_identity)
  WHERE file_identity IS NOT NULL;

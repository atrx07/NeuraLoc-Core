ALTER TABLE messages ADD COLUMN state TEXT NOT NULL DEFAULT 'complete';
ALTER TABLE messages ADD COLUMN job_id TEXT;
ALTER TABLE messages ADD COLUMN usage_json TEXT;
ALTER TABLE messages ADD COLUMN terminal_reason TEXT;
ALTER TABLE messages ADD COLUMN position INTEGER;
ALTER TABLE messages ADD COLUMN updated_at TEXT;

UPDATE messages
SET position = rowid,
    updated_at = created_at
WHERE position IS NULL OR updated_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_job_id
ON messages(job_id)
WHERE job_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_messages_order
ON messages(conversation_id, position, created_at, id);

CREATE INDEX IF NOT EXISTS idx_conversations_list
ON conversations(pinned DESC, updated_at DESC, id DESC);

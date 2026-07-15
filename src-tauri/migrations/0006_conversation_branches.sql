ALTER TABLE conversations
ADD COLUMN source_conversation_id TEXT REFERENCES conversations(id) ON DELETE SET NULL;

ALTER TABLE conversations
ADD COLUMN branch_message_id TEXT REFERENCES messages(id) ON DELETE SET NULL;

ALTER TABLE messages
ADD COLUMN source_message_id TEXT REFERENCES messages(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_conversations_source
ON conversations(source_conversation_id, updated_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_messages_source
ON messages(source_message_id);

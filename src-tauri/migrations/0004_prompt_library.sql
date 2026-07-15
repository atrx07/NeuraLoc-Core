ALTER TABLE prompt_profiles ADD COLUMN created_at TEXT NOT NULL DEFAULT '';
ALTER TABLE prompt_profiles ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';

ALTER TABLE prompt_versions ADD COLUMN raw_document TEXT NOT NULL DEFAULT '';
ALTER TABLE prompt_versions ADD COLUMN source_profile_id TEXT REFERENCES prompt_profiles(id);
ALTER TABLE prompt_versions ADD COLUMN source_version_id TEXT REFERENCES prompt_versions(id);

UPDATE prompt_versions
SET raw_document = content
WHERE raw_document = '';

UPDATE prompt_profiles
SET created_at = COALESCE(
      (SELECT MIN(created_at) FROM prompt_versions WHERE profile_id = prompt_profiles.id),
      ''
    ),
    updated_at = COALESCE(
      (SELECT MAX(created_at) FROM prompt_versions WHERE profile_id = prompt_profiles.id),
      ''
    );

CREATE INDEX IF NOT EXISTS idx_prompt_profiles_library
  ON prompt_profiles(deleted_at, pinned DESC, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_prompt_versions_profile_created
  ON prompt_versions(profile_id, created_at DESC);

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use crate::errors::AppResult;

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "foundation",
        sql: include_str!("../../migrations/0001_foundation.sql"),
    },
    Migration {
        version: 2,
        name: "model_library",
        sql: include_str!("../../migrations/0002_model_library.sql"),
    },
    Migration {
        version: 3,
        name: "engine_packages",
        sql: include_str!("../../migrations/0003_engine_packages.sql"),
    },
    Migration {
        version: 4,
        name: "prompt_library",
        sql: include_str!("../../migrations/0004_prompt_library.sql"),
    },
    Migration {
        version: 5,
        name: "conversation_persistence",
        sql: include_str!("../../migrations/0005_conversation_persistence.sql"),
    },
    Migration {
        version: 6,
        name: "conversation_branches",
        sql: include_str!("../../migrations/0006_conversation_branches.sql"),
    },
];

pub fn run(connection: &mut Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
           version INTEGER PRIMARY KEY,
           name TEXT NOT NULL,
           applied_at TEXT NOT NULL
         );",
    )?;

    for migration in MIGRATIONS {
        let applied = connection
            .query_row(
                "SELECT version FROM schema_migrations WHERE version = ?1",
                [migration.version],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if applied.is_some() {
            continue;
        }

        let transaction = connection.transaction()?;
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, ?3)",
            params![migration.version, migration.name, Utc::now().to_rfc3339()],
        )?;
        transaction.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_is_idempotent() {
        let mut connection = Connection::open_in_memory().unwrap();
        run(&mut connection).unwrap();
        run(&mut connection).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 6);
    }

    #[test]
    fn upgrades_a_version_one_database() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(include_str!("../../migrations/0001_foundation.sql"))
            .unwrap();
        connection
            .execute(
                "INSERT INTO schema_migrations(version, name, applied_at) VALUES (1, 'foundation', ?1)",
                [Utc::now().to_rfc3339()],
            )
            .unwrap();

        run(&mut connection).unwrap();

        let columns: Vec<String> = connection
            .prepare("PRAGMA table_info(models)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(columns.contains(&"verification_state".to_string()));
        assert!(columns.contains(&"gguf_metadata_json".to_string()));
        assert!(columns.contains(&"file_identity".to_string()));
        let engine_package_tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'engine_packages'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(engine_package_tables, 1);
        let prompt_version_columns: Vec<String> = connection
            .prepare("PRAGMA table_info(prompt_versions)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(prompt_version_columns.contains(&"raw_document".to_string()));
        assert!(prompt_version_columns.contains(&"source_profile_id".to_string()));
        assert!(prompt_version_columns.contains(&"source_version_id".to_string()));
        let message_columns: Vec<String> = connection
            .prepare("PRAGMA table_info(messages)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(message_columns.contains(&"state".to_string()));
        assert!(message_columns.contains(&"job_id".to_string()));
        assert!(message_columns.contains(&"usage_json".to_string()));
        assert!(message_columns.contains(&"terminal_reason".to_string()));
        assert!(message_columns.contains(&"position".to_string()));
        assert!(message_columns.contains(&"updated_at".to_string()));
        assert!(message_columns.contains(&"source_message_id".to_string()));
        let conversation_columns: Vec<String> = connection
            .prepare("PRAGMA table_info(conversations)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(conversation_columns.contains(&"source_conversation_id".to_string()));
        assert!(conversation_columns.contains(&"branch_message_id".to_string()));
    }
}

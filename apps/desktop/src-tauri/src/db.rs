//! Small SQLite state layer owned by the host process.
//!
//! The webview never gets SQL. It can only call the narrow commands in
//! `lib.rs`, which go through this module and return JSON values.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRow {
    pub id: i64,
    pub at: String,
    pub text: String,
}

pub struct Db {
    connection: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Db, String> {
        let connection = Connection::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
        let db = Db { connection };
        db.migrate()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Db, String> {
        let db = Db { connection: Connection::open_in_memory().map_err(|error| error.to_string())? };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), String> {
        self.connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 CREATE TABLE IF NOT EXISTS notes (
                     id TEXT PRIMARY KEY,
                     title TEXT NOT NULL,
                     payload TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS events (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     note_id TEXT NOT NULL,
                     at TEXT NOT NULL,
                     text TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS events_by_note ON events(note_id, id);",
            )
            .map_err(|error| format!("migrate: {error}"))
    }

    /// Seeds the sample worklines once. The JSON file is shared with the
    /// frontend's browser fallback, so the Tauri build and a plain Vite preview
    /// show the same shell content.
    pub fn seed(&mut self, json: &str) -> Result<(), String> {
        let notes: Vec<serde_json::Value> =
            serde_json::from_str(json).map_err(|error| format!("seed json: {error}"))?;
        for note in notes {
            let id = note.get("id").and_then(|value| value.as_str()).ok_or("seed note without id")?;
            let title = note.get("title").and_then(|value| value.as_str()).unwrap_or(id);
            self.connection
                .execute(
                    "INSERT OR IGNORE INTO notes (id, title, payload) VALUES (?1, ?2, ?3)",
                    params![id, title, note.to_string()],
                )
                .map_err(|error| format!("seed {id}: {error}"))?;
        }
        Ok(())
    }

    pub fn notes(&self) -> Result<Vec<serde_json::Value>, String> {
        let mut statement =
            self.connection.prepare("SELECT payload FROM notes ORDER BY rowid").map_err(|error| error.to_string())?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0)).map_err(|error| error.to_string())?;
        let mut notes = Vec::new();
        for row in rows {
            let payload: String = row.map_err(|error| error.to_string())?;
            notes.push(serde_json::from_str(&payload).map_err(|error| format!("stored note: {error}"))?);
        }
        Ok(notes)
    }

    pub fn events(&self, note_id: &str) -> Result<Vec<EventRow>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT id, at, text FROM events WHERE note_id = ?1 ORDER BY id")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([note_id], |row| Ok(EventRow { id: row.get(0)?, at: row.get(1)?, text: row.get(2)? }))
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())
    }

    pub fn append_event(&self, note_id: &str, at: &str, text: &str) -> Result<EventRow, String> {
        self.connection
            .execute("INSERT INTO events (note_id, at, text) VALUES (?1, ?2, ?3)", params![note_id, at, text])
            .map_err(|error| format!("append event: {error}"))?;
        Ok(EventRow { id: self.connection.last_insert_rowid(), at: at.to_string(), text: text.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_is_idempotent_and_notes_round_trip() {
        let mut db = Db::open_in_memory().unwrap();
        db.seed(include_str!("../../src/data/notes.json")).unwrap();
        db.seed(include_str!("../../src/data/notes.json")).unwrap();
        let notes = db.notes().unwrap();
        assert!(!notes.is_empty());
        assert!(notes[0].get("title").is_some());
    }

    #[test]
    fn events_append_in_order_per_note() {
        let mut db = Db::open_in_memory().unwrap();
        db.seed(include_str!("../../src/data/notes.json")).unwrap();
        let first = db.append_event("conservation", "今天 15:20", "first").unwrap();
        let second = db.append_event("conservation", "今天 15:21", "second").unwrap();
        assert!(second.id > first.id);
        assert_eq!(db.events("conservation").unwrap().len(), 2);
        assert!(db.events("other").unwrap().is_empty());
    }
}

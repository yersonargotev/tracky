use rusqlite::{Connection, Result};

const REVIEW_FIRST_SCHEMA: &str = include_str!("../migrations/0001_review_first_schema.sql");

/// Apply Tracky's SQLite migrations needed for the review-first import store.
///
/// This storage slice only creates schema. It intentionally does not import PDFs,
/// promote candidates, or connect parser output to persistence yet.
pub fn apply_migrations(connection: &Connection) -> Result<()> {
    connection.execute_batch(REVIEW_FIRST_SCHEMA)
}

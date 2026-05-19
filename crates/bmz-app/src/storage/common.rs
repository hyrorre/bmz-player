use anyhow::Result;
use rusqlite::Connection;

pub fn configure_connection(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", "-65536")?;
    conn.pragma_update(None, "mmap_size", "268435456")?;
    Ok(())
}

use super::*;
use bmz_chart::import::BmsTextEncoding;

type EncodingMap = HashMap<[u8; 32], BmsTextEncoding>;

fn parse_label(label: &str) -> Result<BmsTextEncoding> {
    BmsTextEncoding::from_label(label)
        .ok_or_else(|| anyhow::anyhow!("unknown chart encoding in library: {label}"))
}

impl LibraryDatabase {
    pub(in crate::storage) fn sync_chart_encoding_from(
        &mut self,
        path: &Path,
        sha256: &[u8; 32],
    ) -> Result<()> {
        let mut encoding = None;
        if path.try_exists()? {
            let source = Self::open_read_only(path)?;
            if crate::storage::migration::table_exists(source.conn(), "chart_encoding_revision")? {
                encoding = source.chart_encoding(sha256)?.1;
            }
        }
        if self.chart_encoding(sha256)?.1 != encoding {
            self.set_chart_encoding(sha256, encoding)?;
        }
        Ok(())
    }

    pub fn chart_encoding(&self, sha256: &[u8; 32]) -> Result<(i64, Option<BmsTextEncoding>)> {
        let (revision, label): (i64, Option<String>) = self
            .conn
            .prepare_cached(
                "SELECT revision, (SELECT encoding FROM chart_encoding_overrides WHERE sha256 = ?1)
                 FROM chart_encoding_revision WHERE id = 1",
            )?
            .query_row([hash_to_hex(sha256)], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok((revision, label.as_deref().map(parse_label).transpose()?))
    }

    pub(in crate::storage) fn chart_encodings(&self) -> Result<(i64, EncodingMap)> {
        let mut statement = self.conn.prepare(
            "SELECT revision, sha256, encoding
             FROM chart_encoding_revision LEFT JOIN chart_encoding_overrides ON 1
             WHERE id = 1",
        )?;
        let mut rows = statement.query([])?;
        let mut revision = 0;
        let mut encodings = HashMap::new();
        while let Some(row) = rows.next()? {
            revision = row.get(0)?;
            if let Some(hash) = row.get::<_, Option<String>>(1)? {
                let label: String = row.get(2)?;
                encodings.insert(hex_to_hash(&hash)?, parse_label(&label)?);
            }
        }
        Ok((revision, encodings))
    }

    pub fn set_chart_encoding(
        &mut self,
        sha256: &[u8; 32],
        encoding: Option<BmsTextEncoding>,
    ) -> Result<()> {
        let hash = hash_to_hex(sha256);
        let tx = self.conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let changed = match encoding {
            Some(encoding) => tx.execute(
                "INSERT INTO chart_encoding_overrides(sha256, encoding) VALUES (?1, ?2)
                 ON CONFLICT(sha256) DO UPDATE SET encoding = excluded.encoding
                 WHERE encoding <> excluded.encoding",
                params![hash, encoding.name()],
            )?,
            None => {
                tx.execute("DELETE FROM chart_encoding_overrides WHERE sha256 = ?1", [&hash])?
            }
        };
        if changed != 0 {
            tx.execute(
                "UPDATE chart_encoding_revision SET revision = revision + 1 WHERE id = 1",
                [],
            )?;
            tx.execute("UPDATE charts SET import_version = 0 WHERE sha256 = ?1", [&hash])?;
            // Failed files may have no hash, or a previous successful hash. A negative
            // version makes even an unchanged archive retry its failed entries.
            tx.execute(
                "UPDATE chart_files SET failed_import_version = -1 WHERE parse_status = 'Failed'",
                [],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

pub(in crate::storage) fn check_encoding_revision(conn: &Connection, expected: i64) -> Result<()> {
    anyhow::ensure!(
        encoding_revision_matches(conn, expected)?,
        "chart encoding settings changed; retry the import"
    );
    Ok(())
}

/// Whether imports decoded with the overrides of `expected` may still be written.
/// Check inside the write transaction, after its writes, so no override change can
/// commit between the check and the import.
pub(in crate::storage) fn encoding_revision_matches(
    conn: &Connection,
    expected: i64,
) -> Result<bool> {
    let current: i64 = conn
        .prepare_cached("SELECT revision FROM chart_encoding_revision WHERE id = 1")?
        .query_row([], |row| row.get(0))?;
    Ok(current == expected)
}

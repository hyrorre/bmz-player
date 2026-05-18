use anyhow::{Result, bail};

use crate::cli::TableCommand;
use crate::config::app_config::DifficultyTableSource;
use crate::config::load::load_app_config;
use crate::config::save::save_app_config;
use crate::paths::resolve_app_paths;
use crate::storage::library_db::LibraryDatabase;
use crate::storage::migration::migrate_library_db;

pub async fn run_table_command(cmd: TableCommand) -> Result<()> {
    match cmd {
        TableCommand::Add { url } => add_table(&url).await,
        TableCommand::List => list_tables(),
        TableCommand::Fetch => fetch_tables().await,
    }
}

async fn add_table(url: &str) -> Result<()> {
    let app_paths = resolve_app_paths()?;
    app_paths.ensure_dirs()?;

    let mut app_config = if app_paths.config_toml.exists() {
        load_app_config(&app_paths.config_toml)?
    } else {
        Default::default()
    };

    if app_config.tables.sources.iter().any(|s| s.url == url) {
        bail!("already configured: {url}");
    }
    app_config.tables.sources.push(DifficultyTableSource { url: url.to_string(), enabled: true });
    save_app_config(&app_paths.config_toml, &app_config)?;
    println!("Added {url} to config");

    migrate_library_db(&app_paths.library_db)?;
    let mut library_db = LibraryDatabase::open(&app_paths.library_db)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    println!("Fetching {url}...");
    let table = crate::difficulty_table::fetch_difficulty_table(url, now).await?;
    println!("Fetched: {} ({}) — {} entries", table.name, table.symbol, table.entries.len());
    library_db.upsert_difficulty_table(&table)?;
    println!("Stored.");

    Ok(())
}

fn list_tables() -> Result<()> {
    let app_paths = resolve_app_paths()?;
    migrate_library_db(&app_paths.library_db)?;
    let library_db = LibraryDatabase::open(&app_paths.library_db)?;
    let tables = library_db.list_difficulty_tables()?;

    if tables.is_empty() {
        println!("No difficulty tables stored. Use `table add <URL>` to add one.");
        return Ok(());
    }

    for t in &tables {
        let levels = t.level_order.join(", ");
        println!("{} ({}) — levels: [{}]", t.name, t.symbol, levels);
    }
    Ok(())
}

async fn fetch_tables() -> Result<()> {
    let app_paths = resolve_app_paths()?;
    app_paths.ensure_dirs()?;

    let app_config = if app_paths.config_toml.exists() {
        load_app_config(&app_paths.config_toml)?
    } else {
        Default::default()
    };

    let sources: Vec<_> = app_config.tables.sources.iter().filter(|s| s.enabled).collect();

    if sources.is_empty() {
        println!("No difficulty table sources configured. Use `table add <URL>` to add one.");
        return Ok(());
    }

    migrate_library_db(&app_paths.library_db)?;
    let mut library_db = LibraryDatabase::open(&app_paths.library_db)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut ok = 0usize;
    let mut failed = 0usize;
    for source in &sources {
        print!("Fetching {}... ", source.url);
        match crate::difficulty_table::fetch_difficulty_table(&source.url, now).await {
            Ok(table) => {
                println!("{} ({}) — {} entries", table.name, table.symbol, table.entries.len());
                library_db.upsert_difficulty_table(&table)?;
                ok += 1;
            }
            Err(e) => {
                println!("FAILED: {e}");
                failed += 1;
            }
        }
    }

    println!("\n{ok} succeeded, {failed} failed.");
    Ok(())
}

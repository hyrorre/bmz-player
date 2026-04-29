use anyhow::Result;

pub fn run() -> Result<()> {
    let _boot = crate::bootstrap::bootstrap()?;
    Ok(())
}

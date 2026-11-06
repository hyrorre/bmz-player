use super::*;
use crate::config::profile_config::{SkinAccessGrant, SkinConfig};

#[cfg(test)]
#[path = "skin_access_tests.rs"]
mod tests;

impl AppPaths {
    pub fn skin_access_key(&self, path: &Path) -> Result<String> {
        Ok(normalize_library_path(&std::path::absolute(path)?.to_string_lossy()))
    }

    /// Called for a selected profile entry, never for automatic discovery.
    pub fn skin_path_context(
        &self,
        skin: &SkinConfig,
        path: &Path,
    ) -> Result<bmz_skin::SkinPathContext> {
        self.skin_path_context_with_access(&skin.external_access, path)
    }

    pub fn skin_path_context_with_access(
        &self,
        access: &std::collections::BTreeMap<String, SkinAccessGrant>,
        path: &Path,
    ) -> Result<bmz_skin::SkinPathContext> {
        let mut roots = self.skin_library_roots();
        if let Some(grant) = access.get(&self.skin_access_key(path)?) {
            let real = bmz_skin_assets::canonicalize_skin_path(path)?;
            anyhow::ensure!(
                normalize_library_path(&real.to_string_lossy()) == grant.entry,
                "external skin link target changed; select and authorize the skin again: {}",
                path.display()
            );
            roots.extend(grant.roots.iter().map(PathBuf::from));
            return bmz_skin::SkinPathContext::new(&real, roots);
        }
        bmz_skin::SkinPathContext::new(path, roots)
            .context("skin access denied; use Allow external skin in Skin settings, or add an explicit per-entry external_access grant to profile.toml")
    }

    /// No header read or Lua evaluation occurs during authorization.
    pub fn approve_skin_path(&self, skin: &mut SkinConfig, path: &Path) -> Result<()> {
        let entry = bmz_skin_assets::canonicalize_skin_path(path)?;
        anyhow::ensure!(entry.is_file(), "select a skin file");
        let parent = entry.parent().context("skin entry has no parent")?;
        let mut broad = vec![self.resource_dir.clone(), self.data_dir.clone()];
        broad
            .extend(["HOME", "USERPROFILE"].into_iter().filter_map(env::var_os).map(PathBuf::from));
        broad.extend(
            [&self.resource_dir, &self.data_dir]
                .into_iter()
                .filter_map(|path| path.parent().map(Path::to_path_buf)),
        );
        if let Some(exe_dir) =
            env::current_exe().ok().and_then(|path| path.parent().map(Path::to_path_buf))
        {
            broad.push(exe_dir);
        }
        anyhow::ensure!(
            parent.parent().is_some()
                && !broad.iter().any(|dir| bmz_skin_assets::canonicalize_skin_path(dir)
                    .is_ok_and(|dir| dir == parent)),
            "place this skin in its own folder before authorizing it; the selected parent is too broad"
        );
        skin.external_access.insert(
            self.skin_access_key(path)?,
            SkinAccessGrant {
                entry: normalize_library_path(&entry.to_string_lossy()),
                roots: vec![normalize_library_path(&parent.to_string_lossy())],
            },
        );
        Ok(())
    }
}

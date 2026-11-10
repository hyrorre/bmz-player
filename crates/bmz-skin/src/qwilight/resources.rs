use super::*;

pub(super) struct Resources {
    archive: String,
    names: BTreeMap<String, String>,
}
impl Resources {
    pub fn new(context: &SkinPathContext, eval: &Evaluator) -> Result<Self> {
        let stem = context.entry_file().file_stem().context("skin has no name")?.to_string_lossy();
        let name = eval.definition.text("format", "zip").unwrap_or_else(|| stem.into_owned());
        let archive =
            context.policy().resolve(&context.entry_dir().join(name).with_extension("zip"))?;
        let mut names = BTreeMap::new();
        for name in bmz_skin_assets::zip_entry_names(&archive)? {
            let path = Path::new(&name);
            if path.extension().and_then(|s| s.to_str()).is_some_and(|ext| {
                ["png", "jpg", "jpeg", "bmp", "webp"].contains(&ext.to_ascii_lowercase().as_str())
            }) {
                // A stem such as `.` is a material name, not a filesystem
                // component. Path::with_extension changes `Bin/..png` to
                // `Bin/..`, losing the common decimal-point material.
                let stem = name.rsplit_once('.').expect("image extension checked").0;
                names.entry(stem.replace('\\', "/").to_ascii_lowercase()).or_insert(name);
            }
        }
        Ok(Self { archive: archive.to_string_lossy().to_string(), names })
    }
    pub fn path(&self, name: &str) -> Option<String> {
        self.names
            .get(&name.to_ascii_lowercase())
            .map(|name| format!("{}!skin/{name}", self.archive))
    }
    pub fn sequence(&self, stem: &str, frames: usize, suffix: &str) -> Vec<String> {
        let mut last = self
            .path(&format!("{stem} 0{suffix}"))
            // Only decorations/effects have a frame-less spelling. For notes,
            // dropping the frame makes `kind state` look like `frame kind`.
            .or_else(|| if suffix.is_empty() { self.path(stem) } else { None });
        let mut result = Vec::new();
        for i in 0..frames {
            if let Some(path) = self.path(&format!("{stem} {i}{suffix}")) {
                last = Some(path);
            }
            if let Some(path) = &last {
                result.push(path.clone());
            }
        }
        result
    }
}

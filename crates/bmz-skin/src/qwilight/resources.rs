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
    pub fn frame(&self, stem: &str, frame: usize, suffix: &str) -> Option<String> {
        self.path(&format!("{stem} {frame}{suffix}"))
            .or_else(|| if frame == 0 && suffix.is_empty() { self.path(stem) } else { None })
    }
    /// Resolve callback names for each frame, holding sparse frames without
    /// shifting their time index. Leading gaps remain transparent.
    pub fn animated(
        &self,
        frames: usize,
        mut lookup: impl FnMut(usize) -> Result<Option<String>>,
    ) -> Result<Vec<String>> {
        let mut last = None;
        let mut result = Vec::with_capacity(frames);
        let mut found = false;
        for frame in 0..frames {
            if let Some(path) = lookup(frame)? {
                last = Some(path);
                found = true;
            }
            result.push(last.clone().unwrap_or_else(|| "bmz://lr2/empty".into()));
        }
        Ok(if found { result } else { Vec::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qwilight_sparse_frames_keep_time_indices_and_hold_previous_images() {
        let resources = Resources { archive: String::new(), names: BTreeMap::new() };
        let frames = resources
            .animated(5, |frame| {
                Ok(match frame {
                    1 => Some("first".into()),
                    3 => Some("next".into()),
                    _ => None,
                })
            })
            .unwrap();
        assert_eq!(frames, ["bmz://lr2/empty", "first", "first", "next", "next"]);
        assert!(resources.animated(5, |_| Ok(None)).unwrap().is_empty());
    }
}

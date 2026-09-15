use crate::manifest::{PackageManifest, checked_path, safe_relative};
use anyhow::{Context, Result, ensure};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

/// Extract only a bounded portable package, never directly into the installation.
pub fn extract(
    zip_path: &Path,
    stage: &Path,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<PackageManifest> {
    ensure!(!stage.exists(), "staging directory already exists");
    let mut archive = zip::ZipArchive::new(File::open(zip_path)?)?;
    ensure!(archive.len() <= 150_000, "too many archive entries");
    let manifest: PackageManifest = {
        let mut file = archive.by_name("BMZ Player/bmz-package.json")?;
        ensure!(file.size() <= 16 * 1024 * 1024, "package manifest too large");
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        serde_json::from_slice(&bytes)?
    };
    manifest.validate()?;
    ensure!(manifest.kind == crate::manifest::PackageKind::Portable, "not a portable package");
    let required = manifest.files.iter().map(|entry| entry.size).sum::<u64>() + 64 * 1024 * 1024;
    ensure!(
        crate::process::available_space(stage.parent().context("missing staging parent")?)?
            >= required,
        "insufficient disk space to stage update"
    );
    fs::create_dir_all(stage)?;
    let mut seen = HashSet::new();
    for i in 0..archive.len() {
        checkpoint()?;
        let mut file = archive.by_index(i)?;
        let name =
            file.name().strip_prefix("BMZ Player/").context("unexpected archive root")?.to_owned();
        if name.is_empty() && file.is_dir() {
            continue;
        }
        let relative = name.trim_end_matches('/');
        safe_relative(relative)?;
        ensure!(seen.insert(relative.to_lowercase()), "duplicate archive entry: {name}");
        if let Some(mode) = file.unix_mode() {
            let kind = mode & 0o170000;
            ensure!(
                kind == 0 || kind == 0o100000 || (kind == 0o040000 && file.is_dir()),
                "archive contains special file"
            );
        }
        ensure!(!file.encrypted(), "encrypted package not supported");
        if relative == ".bmz-instance.lock" {
            ensure!(file.size() == 0 && !file.is_dir(), "invalid instance lock");
            continue; // Never replace a live installation's lock file.
        }
        if file.is_dir() {
            continue;
        }
        let expected = if relative == crate::MANIFEST {
            file.size()
        } else {
            manifest
                .files
                .iter()
                .find(|entry| entry.path == relative)
                .context("unmanaged archive file")?
                .size
        };
        ensure!(file.size() == expected, "archive file size mismatch");
        let path = checked_path(stage, relative)?;
        fs::create_dir_all(path.parent().context("missing parent")?)?;
        let mut output = File::options().write(true).create_new(true).open(path)?;
        let mut written = 0u64;
        let mut buf = [0; 64 * 1024];
        loop {
            checkpoint()?;
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            written += n as u64;
            ensure!(written <= expected, "archive expanded beyond declared size");
            output.write_all(&buf[..n])?;
        }
        ensure!(written == expected, "truncated package file");
        output.sync_all()?;
    }
    manifest.verify_files(stage, checkpoint)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{PackageFile, PackageKind};
    use sha2::{Digest, Sha256};
    use zip::write::SimpleFileOptions;

    fn make_zip(path: &Path, extra: Option<(&str, bool)>) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        let bytes = b"executable";
        let manifest = PackageManifest {
            schema: 1,
            kind: PackageKind::Portable,
            target: "windows-x64".into(),
            version: "0.5.0".into(),
            min_updater_protocol: 1,
            files: ["bmz-player.exe", "bmz-updater.exe"]
                .into_iter()
                .map(|name| PackageFile {
                    path: name.into(),
                    size: bytes.len() as u64,
                    sha256: format!("{:x}", Sha256::digest(bytes)),
                })
                .collect(),
        };
        for name in ["bmz-player.exe", "bmz-updater.exe", "bmz-package.json"] {
            zip.start_file(format!("BMZ Player/{name}"), SimpleFileOptions::default()).unwrap();
            if name == "bmz-package.json" {
                zip.write_all(&serde_json::to_vec(&manifest).unwrap()).unwrap();
            } else {
                zip.write_all(bytes).unwrap();
            }
        }
        if let Some((name, symlink)) = extra {
            if symlink {
                zip.add_symlink(name, "../outside", SimpleFileOptions::default()).unwrap();
            } else {
                zip.start_file(name, SimpleFileOptions::default()).unwrap();
                zip.write_all(b"bad").unwrap();
            }
        }
        zip.finish().unwrap();
    }

    #[test]
    fn authentic_layout_extracts_helper_and_rejects_traversal_links_unknown_files() {
        for extra in [
            None,
            Some(("BMZ Player/../outside", false)),
            Some(("BMZ Player/data/score.db", false)),
            Some(("BMZ Player/resources/link", true)),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let zip = temp.path().join("package.zip");
            make_zip(&zip, extra);
            let result = extract(&zip, &temp.path().join("stage"), || Ok(()));
            assert_eq!(result.is_ok(), extra.is_none(), "{extra:?}");
            assert!(!temp.path().join("outside").exists());
        }
    }

    #[test]
    fn cancel_does_not_create_installation_files() {
        let temp = tempfile::tempdir().unwrap();
        let zip = temp.path().join("package.zip");
        make_zip(&zip, None);
        assert!(extract(&zip, &temp.path().join("stage"), || anyhow::bail!("canceled")).is_err());
        assert!(!temp.path().join("bmz-player.exe").exists());
    }
}

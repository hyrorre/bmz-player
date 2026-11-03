use std::path::Path;

/// Filename suffixes shared by discovery, stable resource paths and HTTP downloads.
/// Compound suffixes precede their shorter aliases.
pub const ARCHIVE_EXTENSIONS: &[&str] =
    &["zip", "rar", "7z", "7zip", "tar.gz", "tgz", "lzh", "lha"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveFormat {
    Zip,
    Rar,
    SevenZ,
    TarGz,
    Lzh,
}

impl ArchiveFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?;
        let extension = archive_extension(name)?;
        Some(match extension {
            "zip" => Self::Zip,
            "rar" => Self::Rar,
            "7z" | "7zip" => Self::SevenZ,
            "tar.gz" | "tgz" => Self::TarGz,
            "lzh" | "lha" => Self::Lzh,
            _ => unreachable!(),
        })
    }

    /// A signature is only a routing hint; the selected reader validates the container.
    pub fn from_signature(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
            Some(Self::Zip)
        } else if bytes.starts_with(b"Rar!\x1a\x07") {
            Some(Self::Rar)
        } else if bytes.starts_with(b"7z\xbc\xaf\x27\x1c") {
            Some(Self::SevenZ)
        } else if bytes.starts_with(&[0x1f, 0x8b]) {
            Some(Self::TarGz)
        } else if bytes.get(2..5).is_some_and(|tag| tag == b"-lh" || tag == b"-lz")
            && bytes.get(6) == Some(&b'-')
        {
            Some(Self::Lzh)
        } else {
            None
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Rar => "rar",
            Self::SevenZ => "7z",
            Self::TarGz => "tar.gz",
            Self::Lzh => "lzh",
        }
    }
}

pub fn archive_extension(name: &str) -> Option<&'static str> {
    ARCHIVE_EXTENSIONS.iter().copied().find(|extension| {
        name.len() > extension.len() + 1
            && name.as_bytes().get(name.len() - extension.len() - 1) == Some(&b'.')
            && name
                .get(name.len() - extension.len()..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(extension))
    })
}

pub fn archive_stem(name: &str) -> Option<&str> {
    let extension = archive_extension(name)?;
    name.get(..name.len() - extension.len() - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffixes_aliases_and_compound_stems_match() {
        for (name, format, stem) in [
            ("曲.ZIP", ArchiveFormat::Zip, "曲"),
            ("v1.2.RAR", ArchiveFormat::Rar, "v1.2"),
            ("set.7zip", ArchiveFormat::SevenZ, "set"),
            ("set.TAR.GZ", ArchiveFormat::TarGz, "set"),
            ("set.tgz", ArchiveFormat::TarGz, "set"),
            ("古い曲.LHA", ArchiveFormat::Lzh, "古い曲"),
        ] {
            assert_eq!(ArchiveFormat::from_path(Path::new(name)), Some(format));
            assert_eq!(archive_stem(name), Some(stem));
        }
        for name in ["song.gz", "zip", "a.bms", "曲gz", "a.zip.bak"] {
            assert_eq!(ArchiveFormat::from_path(Path::new(name)), None);
        }
    }
}

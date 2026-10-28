//! Discovery through links and deterministic link choice.

use std::io::Write;

use super::*;
use crate::storage::library_db::library_path_key;

fn config(follow_symlinks: bool) -> ScanConfig {
    ScanConfig {
        follow_symlinks,
        skip_hidden: true,
        use_everything: false,
        auto_rescan_on_startup: false,
        rescan_missing_files: true,
    }
}

fn temp() -> tempfile::TempDir {
    tempfile::Builder::new().prefix("bmz-scan-links-").tempdir().unwrap()
}

fn chart(path: &Path, title: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let mut file = std::fs::File::create(path).unwrap();
    write!(file, "#TITLE {title}\n#BPM 120\n#00011:01\n").unwrap();
}

fn discovered(root: &Path, recursive: bool, config: &ScanConfig) -> (Vec<String>, ChartDiscovery) {
    let discovery = discover_chart_files_with_progress(root, recursive, config, |_| {});
    let mut paths: Vec<String> = discovery
        .entries
        .iter()
        .map(|entry| {
            let key = library_path_key(&entry.path);
            key.strip_prefix(&format!("{}/", library_path_key(root))).unwrap().to_string()
        })
        .collect();
    paths.sort();
    (paths, discovery)
}

#[cfg(unix)]
fn link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(unix)]
#[test]
fn link_to_an_ancestor_of_the_root_is_not_followed() {
    let base = temp();
    let root = base.path().join("music/songs/root");
    chart(&root.join("a/a.bms"), "A");
    chart(&base.path().join("music/other/x.bms"), "Outside");
    link(&base.path().join("music"), &root.join("up"));
    link(&root, &root.join("a/self"));

    let (paths, discovery) = discovered(&root, true, &config(true));

    assert_eq!(paths, ["a/a.bms"]);
    assert!(discovery.complete);
}

#[cfg(unix)]
#[test]
fn link_to_a_directory_inside_the_root_keeps_the_real_path() {
    let base = temp();
    let root = base.path().join("root");
    chart(&root.join("inner/a.bms"), "A");
    link(&root.join("inner"), &root.join("alias"));
    link(&root.join("inner"), &root.join("inner/again"));
    // A file link to a chart that is read through a real folder.
    link(&root.join("inner/a.bms"), &root.join("a-link.bms"));

    let (paths, discovery) = discovered(&root, true, &config(true));

    assert_eq!(paths, ["inner/a.bms"]);
    assert!(discovery.complete);
}

#[cfg(unix)]
#[test]
fn overlapping_external_links_read_every_folder_once() {
    let base = temp();
    let ext = base.path().join("ext");
    chart(&ext.join("c.bms"), "C");
    chart(&ext.join("sub/b.bms"), "B");

    // The descendant is chosen first; the ancestor is then read without it.
    let root = base.path().join("root1");
    std::fs::create_dir_all(&root).unwrap();
    link(&ext.join("sub"), &root.join("l1"));
    link(&ext, &root.join("l2"));
    assert_eq!(discovered(&root, true, &config(true)).0, ["l1/b.bms", "l2/c.bms"]);

    // The ancestor is chosen first; the descendant link is then already covered.
    let root = base.path().join("root2");
    std::fs::create_dir_all(&root).unwrap();
    link(&ext, &root.join("a"));
    link(&ext.join("sub"), &root.join("b"));
    assert_eq!(discovered(&root, true, &config(true)).0, ["a/c.bms", "a/sub/b.bms"]);

    // A link found inside an adopted tree that points at its ancestor.
    let root = base.path().join("root3");
    std::fs::create_dir_all(&root).unwrap();
    link(&ext.join("sub"), &root.join("a"));
    link(&ext, &ext.join("sub/parent"));
    assert_eq!(discovered(&root, true, &config(true)).0, ["a/b.bms", "a/parent/c.bms"]);
}

#[cfg(unix)]
#[test]
fn links_to_the_same_folder_choose_the_same_path_every_scan() {
    let base = temp();
    let ext = base.path().join("ext");
    chart(&ext.join("song.bms"), "Shared");
    let root = base.path().join("root");
    std::fs::create_dir_all(root.join("deep")).unwrap();
    // Created in an order unrelated to the expected choice.
    link(&ext, &root.join("linkB"));
    link(&ext, &root.join("deep/linkA"));
    link(&ext, &root.join("linkA"));

    for _ in 0..20 {
        assert_eq!(discovered(&root, true, &config(true)).0, ["linkA/song.bms"]);
    }
    // A different thread count must not change the choice either.
    let pool = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
    pool.install(|| assert_eq!(discovered(&root, true, &config(true)).0, ["linkA/song.bms"]));
}

#[cfg(unix)]
#[test]
fn cyclic_links_terminate_without_duplicates_or_incomplete_scans() {
    let base = temp();
    let root = base.path().join("root");
    let ext = base.path().join("ext");
    chart(&root.join("r.bms"), "Root");
    chart(&ext.join("e.bms"), "Ext");
    chart(&ext.join("nested/n.bms"), "Nested");
    link(&ext, &root.join("ext"));
    link(&root, &ext.join("back"));
    link(&ext, &ext.join("nested/self"));
    // Links that only resolve through each other.
    link(&root.join("loop-b"), &root.join("loop-a"));
    link(&root.join("loop-a"), &root.join("loop-b"));

    let (paths, discovery) = discovered(&root, true, &config(true));

    assert_eq!(paths, ["ext/e.bms", "ext/nested/n.bms", "r.bms"]);
    assert!(discovery.complete);
    assert!(discovery.issues.is_empty());
}

#[cfg(unix)]
#[test]
fn real_folder_wins_over_a_link_reaching_the_same_charts() {
    let base = temp();
    let root = base.path().join("root");
    let ext = base.path().join("ext");
    chart(&root.join("real/a.bms"), "A");
    chart(&ext.join("e.bms"), "E");
    link(&ext, &root.join("ext"));
    link(&root.join("real"), &ext.join("to-real"));
    link(&root.join("real/a.bms"), &ext.join("a.bms"));

    assert_eq!(discovered(&root, true, &config(true)).0, ["ext/e.bms", "real/a.bms"]);
}

fn write_zip(path: &Path, entries: &[(&str, &str)]) {
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, text) in entries {
        writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(text.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
}

#[cfg(unix)]
#[test]
fn linked_archive_is_read_once() {
    let base = temp();
    let root = base.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    write_zip(&root.join("pack.zip"), &[("a.bms", "#TITLE A\n#BPM 120\n#00011:01\n")]);
    link(&root.join("pack.zip"), &root.join("alias.zip"));

    assert_eq!(discovered(&root, true, &config(true)).0, ["pack.zip!/a.bms"]);
}

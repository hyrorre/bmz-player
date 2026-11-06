use super::*;

#[test]
fn external_permission_is_explicit_per_entry_persistent_and_revocable() {
    let root = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_dirs(
        root.path().join("resource"),
        root.path().join("data"),
        root.path().join("cache"),
        root.path().join("logs"),
    );
    let library = paths.data_dir.join("skins");
    let external = root.path().join("external");
    for dir in [&library, &external] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let entry = external.join("select.lua");
    let other = external.join("other.lua");
    for path in [&entry, &other, &library.join("inside.lua")] {
        std::fs::write(path, "return {type=5}").unwrap();
    }
    let mut skin = SkinConfig::default();
    let loose = root.path().join("loose.lua");
    std::fs::write(&loose, "return {}").unwrap();
    assert!(paths.approve_skin_path(&mut skin, &loose).is_err());
    assert!(paths.skin_path_context(&skin, &entry).is_err());
    assert!(paths.skin_path_context(&skin, &library.join("inside.lua")).is_ok());
    paths.approve_skin_path(&mut skin, &entry).unwrap();
    let restored: SkinConfig = toml::from_str(&toml::to_string(&skin).unwrap()).unwrap();
    let context = paths.skin_path_context(&restored, &entry).unwrap();
    assert!(context.resolve_file("other.lua").is_ok());
    assert!(paths.skin_path_context(&restored, &other).is_err());
    assert!(bmz_skin::SkinPathContext::new(&entry, paths.skin_library_roots()).is_err());
    assert!(context.resolve_path_or_missing("../unapproved/new.txt").is_err());
    skin.external_access.clear();
    assert!(paths.skin_path_context(&skin, &entry).is_err());
}

#[cfg(unix)]
#[test]
fn changing_an_authorized_entry_link_requires_new_permission() {
    let root = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_dirs(
        root.path().join("resource"),
        root.path().join("data"),
        root.path().join("cache"),
        root.path().join("logs"),
    );
    let external = root.path().join("external");
    std::fs::create_dir(&external).unwrap();
    for name in ["one.lua", "two.lua"] {
        std::fs::write(external.join(name), "return {}").unwrap();
    }
    let link = root.path().join("chosen.lua");
    std::os::unix::fs::symlink(external.join("one.lua"), &link).unwrap();
    let mut skin = SkinConfig::default();
    paths.approve_skin_path(&mut skin, &link).unwrap();
    assert!(paths.skin_path_context(&skin, &link).is_ok());
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(external.join("two.lua"), &link).unwrap();
    assert!(
        paths.skin_path_context(&skin, &link).unwrap_err().to_string().contains("target changed")
    );
}

use super::*;

#[test]
fn discovery_never_authorizes_an_external_entry() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("skins");
    let other = temp.path().join("skins-other");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&other).unwrap();
    let entry = other.join("external.luaskin");
    fs::write(&entry, "return {}").unwrap();
    assert!(SkinPathContext::new(&entry, [root.clone()]).is_err());
    let explicit = SkinPathContext::for_entry(&entry).unwrap();
    assert_eq!(explicit.library_roots(), &[canonicalize_skin_path(&other).unwrap()]);
    assert!(SkinPathContext::new(&entry, []).is_err());
    assert!(explicit.resolve_path("").is_err());
    assert!(explicit.resolve_path("nul\0.txt").is_err());
    assert!(explicit.resolve_path("C:secret.txt").is_err());
    // Reject unknown UNC roots before any network filesystem lookup.
    assert!(explicit.resolve_path("//unapproved.invalid/share/secret.txt").is_err());
    assert!(explicit.resolve_path_or_missing("../secret.txt").is_err());
}

#[test]
fn explicit_shared_roots_are_available_without_parent_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("skins");
    let shared = temp.path().join("shared");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&shared).unwrap();
    let entry = root.join("skin.json");
    fs::write(&entry, "{}").unwrap();
    fs::write(shared.join("page.png"), "image").unwrap();
    let context = SkinPathContext::new(&entry, [root.clone(), shared.clone()]).unwrap();
    assert_eq!(
        context.resolve_file("../shared/page.png").unwrap(),
        canonicalize_skin_path(&shared.join("page.png")).unwrap()
    );
    assert!(
        SkinPathContext::for_entry(&entry).unwrap().resolve_file("../shared/page.png").is_err()
    );
}

#[test]
fn wildcard_choices_keep_case_insensitive_and_unicode_names() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Prefix日本語.PNG"), "image").unwrap();
    let context = SkinPathContext::for_directory(root.path()).unwrap();
    assert_eq!(context.wildcard_candidate_values("prefix*.png").unwrap(), vec!["prefix日本語.png"]);
    assert!(context.resolve_selected_for_pattern("prefix*.png", "prefix日本語.png").is_some());
}

#[cfg(unix)]
#[test]
fn links_are_checked_for_existing_missing_wildcard_and_archive_paths() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("skins");
    let outside = temp.path().join("outside");
    fs::create_dir_all(root.join("inside")).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(root.join("skin.json"), "{}").unwrap();
    fs::write(root.join("inside/good.png"), "image").unwrap();
    fs::write(outside.join("secret.png"), "private").unwrap();
    fs::write(outside.join("private.dxa"), "must not parse").unwrap();
    symlink(&outside, root.join("escape")).unwrap();
    symlink(outside.join("secret.png"), root.join("bad.png")).unwrap();
    symlink(root.join("inside/good.png"), root.join("good.png")).unwrap();
    symlink(root.join("inside"), root.join("alias")).unwrap();
    symlink(temp.path().join("absent"), root.join("broken")).unwrap();
    symlink("loop", root.join("loop")).unwrap();
    symlink(&root, temp.path().join("linked-root")).unwrap();
    let context = SkinPathContext::new(
        &temp.path().join("linked-root/skin.json"),
        [temp.path().join("linked-root")],
    )
    .unwrap();
    assert!(context.resolve_file("good.png").is_ok());
    assert!(
        context
            .resolve_path_or_missing("alias/new/log.txt")
            .unwrap()
            .starts_with(canonicalize_skin_path(&root.join("inside")).unwrap())
    );
    for path in ["escape/secret.png", "bad.png", "broken", "loop"] {
        assert!(context.resolve_path(path).is_err(), "{path}");
        assert!(context.resolve_optional_asset(path).is_err(), "{path}");
    }
    for path in ["escape/new/log.txt", "broken/new", "loop/new"] {
        assert!(context.resolve_path_or_missing(path).is_err(), "{path}");
    }
    assert!(context.resolve_asset("escape/private/atlas.png").is_err());
    assert!(context.resolve_asset("escape/SECRET.PNG").is_err());
    for path in ["ESCAPE/missing.png", "escape/new/log.txt", "broken/new", "loop/new"] {
        assert!(context.resolve_optional_asset(path).is_err(), "{path}");
    }
    assert_eq!(
        context.wildcard_candidates("*.png").unwrap(),
        vec![canonicalize_skin_path(&root.join("inside/good.png")).unwrap()]
    );
}

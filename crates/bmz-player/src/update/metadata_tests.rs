use super::*;

const FIXTURE: &[u8] = include_bytes!("../../../bmz-updater/tests/fixtures/release.json");
// Ed25519 verification key for the public fixture seed [7; 32].
const KEY: &str = "6kpsY+KcUgq+9VB7Ey7F+ZVHdq6+vnuSQh7qaRRG0iw=";

fn metadata(name: &str) -> GithubAsset {
    GithubAsset { name: name.into(), browser_download_url: String::new(), size: 1, digest: None }
}

#[test]
fn prefers_release_and_only_falls_back_when_absent() {
    let old = metadata("updates.json");
    assert_eq!(update_metadata_asset(std::slice::from_ref(&old)).unwrap().name, "updates.json");
    let assets = [old, metadata("release.json")];
    let selected = update_metadata_asset(&assets).unwrap();
    assert_eq!(selected.name, "release.json");
    assert!(verify_update_metadata(&selected.name, b"invalid", KEY).is_err());
    assert!(update_metadata_asset(&[metadata("SHA256SUMS.txt")]).is_none());
    assert_eq!(verify_update_metadata("release.json", FIXTURE, KEY).unwrap().packages.len(), 2);
    assert!(verify_update_metadata("updates.json", FIXTURE, KEY).is_err());
}

#[test]
fn bridge_chain_upgrades_reader_without_raising_bridge_package_requirements() {
    let packages = verify_update_metadata("release.json", FIXTURE, KEY).unwrap().packages;
    for kind in [PackageKind::Portable, PackageKind::Installer] {
        let installed = PackageManifest {
            schema: 1,
            kind,
            target: "windows-x64".into(),
            version: "0.4.0".into(),
            min_updater_protocol: 1,
            files: vec![],
        };
        let package = packages.iter().find(|p| p.kind == kind).unwrap().clone();
        let release = |version: &str, min_updater_protocol, bridge: Option<&str>| {
            let mut next = package.clone();
            next.version = version.into();
            next.min_updater_protocol = min_updater_protocol;
            next.bridge_tag = bridge.map(str::to_owned);
            vec![next]
        };
        // C -> B -> A when the running application still supports only protocol 1.
        assert!(
            matches!(update_step(release("0.5.0", 3, Some("v0.4.4")), "v0.5.0", &installed, "0.4.0", 1).unwrap(), Some(UpdateStep::Bridge(tag)) if tag == "v0.4.4")
        );
        assert!(
            matches!(update_step(release("0.4.4", 2, Some("v0.4.3")), "v0.4.4", &installed, "0.4.0", 1).unwrap(), Some(UpdateStep::Bridge(tag)) if tag == "v0.4.3")
        );
        assert!(matches!(
            update_step(release("0.4.3", 1, None), "v0.4.3", &installed, "0.4.0", 1).unwrap(),
            Some(UpdateStep::Package(_))
        ));
        // After A, B installs with protocol 2; after B, C installs with protocol 3.
        assert!(matches!(
            update_step(release("0.4.4", 2, Some("v0.4.3")), "v0.4.4", &installed, "0.4.3", 2)
                .unwrap(),
            Some(UpdateStep::Package(_))
        ));
        assert!(matches!(
            update_step(release("0.5.0", 3, Some("v0.4.4")), "v0.5.0", &installed, "0.4.4", 3)
                .unwrap(),
            Some(UpdateStep::Package(_))
        ));
        assert!(
            update_step(release("0.5.0", 3, Some("v0.4.3")), "v0.5.0", &installed, "0.4.3", 2)
                .is_err()
        );
        assert!(
            update_step(release("0.5.0", 3, Some("v0.5.0")), "v0.5.0", &installed, "0.4.3", 2)
                .is_err()
        );
        assert!(
            update_step(release("0.5.0", 3, Some("v0.4.4")), "v0.5.1", &installed, "0.4.3", 2)
                .is_err()
        );
    }
}

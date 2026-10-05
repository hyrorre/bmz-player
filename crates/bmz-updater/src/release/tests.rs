use super::*;
use ed25519_dalek::{Signer, SigningKey};

const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/release.json");

fn public_key() -> String {
    STANDARD.encode(SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes())
}

fn signed(mut value: Value) -> Vec<u8> {
    value.as_object_mut().unwrap().remove("signature");
    let bytes = canonical_json(&value).unwrap();
    value["signature"] =
        STANDARD.encode(SigningKey::from_bytes(&[7; 32]).sign(bytes.as_bytes()).to_bytes()).into();
    serde_json::to_vec(&value).unwrap()
}

#[test]
fn verifies_node_fixture_and_projects_only_windows_packages() {
    let release = verify_release(FIXTURE, &public_key()).unwrap();
    assert_eq!(release.builds.len(), 5);
    assert_eq!(release.artifacts.len(), 7);
    let updates = release.windows_updates();
    assert_eq!(updates.packages.len(), 2);
    assert_eq!(updates.packages[0].kind, PackageKind::Portable);
    assert_eq!(updates.packages[1].kind, PackageKind::Installer);
    assert!(updates.packages.iter().all(|package| package.min_updater_protocol == 2));
    // Whitespace and object member order are not part of the signature.
    let json: Value = serde_json::from_slice(FIXTURE).unwrap();
    verify_release(&serde_json::to_vec(&json).unwrap(), &public_key()).unwrap();
}

#[test]
fn rejects_tampering_wrong_keys_missing_signatures_and_duplicate_fields() {
    let original: Value = serde_json::from_slice(FIXTURE).unwrap();
    for pointer in [
        "/version",
        "/git_commit",
        "/builds/0/client_hash",
        "/artifacts/0/sha256",
        "/artifacts/0/url",
        "/artifacts/0/update/bridge_tag",
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = Value::String("tampered".into());
        assert!(
            verify_release(&serde_json::to_vec(&changed).unwrap(), &public_key()).is_err(),
            "{pointer}"
        );
    }
    assert!(
        verify_release(
            FIXTURE,
            &STANDARD.encode(SigningKey::from_bytes(&[8; 32]).verifying_key().to_bytes())
        )
        .is_err()
    );
    let mut unsigned = original.clone();
    unsigned["signature"] = Value::Null;
    assert!(verify_release(&serde_json::to_vec(&unsigned).unwrap(), &public_key()).is_err());
    let text = std::str::from_utf8(FIXTURE).unwrap();
    let duplicate = text.replacen("\"version\":", "\"version\":\"0.5.0\",\"version\":", 1);
    assert!(verify_release(duplicate.as_bytes(), &public_key()).is_err());
    let duplicate = text.replacen("\"sha256\":", "\"sha256\":\"wrong\",\"sha256\":", 1);
    assert!(verify_release(duplicate.as_bytes(), &public_key()).is_err());
    let mut unknown = original.clone();
    unknown["ignored"] = true.into();
    assert!(verify_release(&signed(unknown), &public_key()).is_err());
}

#[test]
fn rejects_signed_but_invalid_inventories_and_update_policy() {
    let original: Value = serde_json::from_slice(FIXTURE).unwrap();
    for (pointer, value) in [
        ("/schema", "other".into()),
        ("/client", "other".into()),
        ("/git_commit", "z".repeat(40).into()),
        ("/builds/0/id", "macos-x64".into()),
        ("/builds/0/client_hash", "B".repeat(64).into()),
        ("/artifacts/0/build", "linux-x64-tar".into()),
        ("/artifacts/0/name", "../app.zip".into()),
        ("/artifacts/0/url", "https://example.com/app.zip".into()),
        ("/artifacts/0/size", 0.into()),
        ("/artifacts/0/size", 2_147_483_648_u64.into()),
        ("/artifacts/0/update/min_updater_protocol", 0.into()),
        ("/artifacts/0/update/bridge_tag", "v0.5.0".into()),
        ("/artifacts/0/update/bridge_tag", Value::Null),
        ("/artifacts/0/update", Value::Null),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert!(verify_release(&signed(changed), &public_key()).is_err(), "{pointer}");
    }
    let mut incomplete = original;
    incomplete["artifacts"].as_array_mut().unwrap().pop();
    assert!(verify_release(&signed(incomplete), &public_key()).is_err());
}

#[test]
fn canonical_profile_uses_utf16_key_order_and_exact_unsigned_integers() {
    assert_eq!(
        canonical_json(&serde_json::json!({"\u{e000}": 2, "😀": 1})).unwrap(),
        "{\"😀\":1,\"\u{e000}\":2}"
    );
    assert_eq!(
        canonical_json(&serde_json::json!({"z": "\u{f}日本語", "a": [true, null, 123]})).unwrap(),
        "{\"a\":[true,null,123],\"z\":\"\\u000f日本語\"}"
    );
    for json in ["1.5", "-1", "-0.0", "9007199254740992"] {
        assert!(canonical_json(&serde_json::from_str(json).unwrap()).is_err());
    }
}

use super::*;

#[test]
fn bit32_never_replaces_an_existing_global() {
    let lua = Lua::new();
    let existing = lua.create_table().unwrap();
    existing.set("custom", true).unwrap();
    lua.globals().set("bit32", existing).unwrap();
    install_bit32_library(&lua).unwrap();
    let bit32: Table = lua.globals().get("bit32").unwrap();
    assert!(bit32.get::<bool>("custom").unwrap());
    assert!(matches!(bit32.get::<Value>("band").unwrap(), Value::Nil));
}

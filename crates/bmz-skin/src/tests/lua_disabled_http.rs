use super::*;

#[test]
fn disabled_async_http_has_beatoraja_failure_values_in_every_vm() {
    let root = TestDir::created("bmz-lua-disabled-http");
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        r#"
        local s = require('main_state')
        local function status()
            local id, err = s.http_get_async('https://example.invalid/', 1000, 1024)
            assert(id == nil and err == 'HTTP disabled')
            local response = s.http_poll(1)
            assert(response.status == 'failed' and response.status_code == 0)
            assert(#response.lines == 0 and response.error == err)
            assert(s.http_cancel(1) == false)
            return err
        end
        return {type=5, name=status(), text={{id='status', value=status}}}
    "#,
    )
    .unwrap();
    let mut loaded = load_lua_skin_with_runtime_state(
        &path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
    )
    .unwrap();
    assert_eq!(loaded.document.name, "HTTP disabled");
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let id = (0..runtime.callback_count())
        .find(|&id| runtime.callback_path(id) == Some("$.text[1].value"))
        .unwrap();
    assert_eq!(
        runtime.evaluate_text(id, &TestLuaMainState::default()).as_deref(),
        Some("HTTP disabled")
    );
    assert_eq!(runtime.failure_log_count(), 0);
}

#[test]
fn installed_social_http_wrapper_finishes_a_disabled_request_once_when_available() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/SocialSkin/lua/modules/commons");
    if !source.join("http_request.lua").is_file() {
        return;
    }
    let root = TestDir::created("bmz-social-disabled-http");
    let modules = root.join("modules/commons");
    fs::create_dir_all(&modules).unwrap();
    for file in ["http_request.lua", "compat.lua"] {
        fs::copy(source.join(file), modules.join(file)).unwrap();
    }
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        r#"
        local http = require('modules.commons.http_request')
        local request = http.get('https://example.invalid/', 1000, 1024)
        local calls, result, message = 0, nil, nil
        request:runHttpRequest(function(success, lines, err)
            calls = calls + 1; result = success; message = err
        end)
        assert(request.isConnecting)
        request:update()
        request:update()
        assert(calls == 1 and result == false and message == 'HTTP disabled')
        assert(request.isConnecting == false and request.wasSuccess == false)
        return {type=5, name=message}
    "#,
    )
    .unwrap();
    let loaded =
        load_lua_skin(&path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert_eq!(loaded.document.name, "HTTP disabled");
}

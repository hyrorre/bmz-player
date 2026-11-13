//! 設定フォルダの操作と、プレー中の 1P / 2P 割り当て (WinitApp を作るテスト)。

use super::*;

fn two_controller_app(
    event_loop: &EventLoop<AppUserEvent>,
    data: &crate::bootstrap::profile_tests::ProfileTestDir,
) -> (Box<WinitApp>, DeviceId, DeviceId) {
    let mut boot = data.boot();
    boot.app_config.input.gamepad_enabled = false;
    boot.app_config.input.gamepad_slot_device_ids =
        [Some("test:left".into()), Some("test:right".into())];
    let left = crate::input::gamepad::gamepad_device_id_from_stable_id("test:left");
    let right = crate::input::gamepad::gamepad_device_id_from_stable_id("test:right");
    let (maintenance_tx, _) = tokio::sync::watch::channel(false);
    let mut app = WinitApp::new(
        boot,
        AppOptions { viewer_play: true, ..Default::default() },
        Instant::now(),
        None,
        None,
        Arc::new(AtomicBool::new(false)),
        event_loop.create_proxy(),
        LogBuffer::default(),
        maintenance_tx,
        None,
    )
    .unwrap();
    app.viewer_mode = false;
    (app, left, right)
}

#[test]
#[ignore = "creates a Winit event loop; run alone with --ignored"]
fn settings_folder_ignores_e2_while_browsing_editing_and_listening() {
    use crate::config::key_config::{
        KeyBindingSlot, KeyBindingTarget, apply_play_binding, format_play_binding,
    };
    use crate::screens::settings_model::{CONFIG_ROOT_PATH, load_settings_items};
    use winit::platform::windows::EventLoopBuilderExtWindows;

    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    let (mut app, left, right) = two_controller_app(&event_loop, &data);
    // 1P の E2 を 7K KEY1 と同じ Button1 にする。2P は共通の E2 (Button10)。
    apply_play_binding(
        &mut app.boot.profile_config.input,
        KeyMode::K7,
        KeyBindingTarget::Action {
            action: InputActionConfig::E2,
            slot: KeyBindingSlot::Controller1P,
        },
        "Button1",
    )
    .unwrap();
    app.test_apply_input_config();
    let path = format!("{CONFIG_ROOT_PATH}volume");
    app.select.folder_stack = vec![path.clone()];
    app.select.select_items = load_settings_items(&path);
    app.select.selected_index = 1;
    assert!(matches!(app.select.select_items[1], SelectItem::Config(_)));
    let press = |app: &mut WinitApp, device: DeviceId, button: &str| {
        let input = app.test_device_input(
            device,
            &PhysicalControl::GamepadButton(button.into()),
            true,
            false,
        );
        app.route_settings_control(&input);
    };
    let unchanged = |app: &WinitApp| {
        assert!(!app.select.list_loader.loading(), "フォルダを閉じない");
        assert_eq!(app.select.folder_stack, vec![path.clone()]);
    };

    // 閲覧: E2 では戻らない。E2 と同じボタンの KEY1 でも確定しない。
    press(&mut app, right, "Button10");
    unchanged(&app);
    press(&mut app, left, "Button1");
    unchanged(&app);
    assert!(app.select.settings_edit.is_none(), "E2 を KEY1 として確定しない");
    assert_eq!(app.select.selected_index, 1);

    // 値編集: 2P の KEY1 で編集を始め、E2 ではキャンセル・確定しない。
    press(&mut app, right, "Button1");
    assert!(app.select.settings_edit.is_some());
    press(&mut app, right, "Button10");
    press(&mut app, left, "Button1");
    assert!(app.select.settings_edit.is_some(), "E2 では編集を終えない");
    // 偶数鍵でキャンセル。
    press(&mut app, right, "Button2");
    assert!(app.select.settings_edit.is_none());
    unchanged(&app);

    // キーコンフィグの入力待ち: E2 は戻る操作ではなく、押したボタンとして割り当てる。
    let target = KeyBindingTarget::Key { lane: LaneConfig::Key3, slot: KeyBindingSlot::Controller };
    app.begin_key_config_edit(KeyMode::K7, target);
    app.route_gamepad_button_event(&crate::input::gamepad::GamepadButtonEvent {
        name: "Button10".into(),
        device_id: right,
        pressed: true,
        timestamp: bmz_gameplay::input::backend::DeviceTimestamp::MonotonicNs(0),
        synthesized_analog_axis: false,
    });
    assert!(app.select.key_config_edit.is_none());
    assert_eq!(format_play_binding(&app.boot.profile_config, KeyMode::K7, target), "Button10");
    unchanged(&app);

    // 入力待ちでない状態の E2 もキャンセルしない。偶数鍵でキャンセルする。
    app.begin_key_config_edit(KeyMode::K7, target);
    if let Some(session) = app.select.key_config_edit.as_mut() {
        session.listening = false;
    }
    press(&mut app, right, "Button10");
    assert!(app.select.key_config_edit.is_some());
    press(&mut app, right, "Button2");
    assert!(app.select.key_config_edit.is_none());

    // 偶数鍵でフォルダを閉じる。
    press(&mut app, left, "Button2");
    assert!(app.select.list_loader.loading() || app.select.folder_stack.is_empty());
}

#[test]
#[ignore = "creates a Winit event loop; run alone with --ignored"]
fn play_in_progress_resolves_input_with_the_play_start_slots() {
    use winit::platform::windows::EventLoopBuilderExtWindows;

    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    let (mut app, left, right) = two_controller_app(&event_loop, &data);
    app.test_apply_input_config();
    let start_slots = app.input.gamepad_slots;
    assert_eq!(start_slots.slot_device_ids, [Some(left), Some(right)]);
    // 読み込み中のプレー。
    app.play.pending_play_start = Some(PendingPlayStart::from_snapshot(
        1,
        Default::default(),
        &RenderSnapshot::default(),
        &app.boot.profile_config,
        KeyMode::K14,
        KeyMode::K14,
        start_slots,
    ));
    app.play.play_option_input = Some(PlayOptionInput::new(
        KeyMode::K14,
        crate::config::play::lane_binding_for_chart_with_slots(
            &app.boot.profile_config.input,
            KeyMode::K14,
            start_slots,
        ),
        &app.boot.profile_config.input,
        start_slots,
    ));

    // 読み込み中に左右が入れ替わった。
    let swapped = crate::input::gamepad::GamepadSlotMap::from_device_ids([Some(right), Some(left)]);
    app.update_gamepad_slot_assignment(swapped);
    assert_eq!(app.input_slots(), start_slots);
    let button1 = PhysicalControl::GamepadButton("Button1".into());
    let input = app.test_device_input(right, &button1, true, false);
    assert_eq!(input.scene().slot, Some(input_resolve::PlayerSide::P2));
    assert_eq!(input.play().and_then(|play| play.slot), Some(input_resolve::PlayerSide::P2));
    // 次のラウンド (Practice・リトライ) のセッションも開始時の割り当てを引き継ぐ。
    assert_eq!(
        app.play_session_app_config().input.gamepad_slot_runtime_device_ids,
        start_slots.slot_device_ids.map(|id| id.map(|id| id.0))
    );

    // 選曲へ戻ると、更新後の割り当てを使う。
    app.play.pending_play_start = None;
    assert_eq!(app.input_slots(), swapped);
    let input = app.test_device_input(right, &button1, true, false);
    assert_eq!(input.scene().slot, Some(input_resolve::PlayerSide::P1));
    assert_eq!(app.play_session_gamepad_slots(), None);
}

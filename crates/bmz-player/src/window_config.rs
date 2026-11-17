use std::collections::HashMap;

use winit::monitor::MonitorHandle;

pub fn list_monitors() -> anyhow::Result<()> {
    struct Listing;
    impl winit::application::ApplicationHandler for Listing {
        fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
            let primary = event_loop.primary_monitor();
            for monitor in describe_monitors(event_loop.available_monitors()) {
                let size = monitor.handle.size();
                crate::stdio::stdout_line(format_args!(
                    "{}\t{}x{}\t{}{}",
                    monitor.facts.config_value(),
                    size.width,
                    size.height,
                    monitor.facts.label(),
                    if Some(&monitor.handle) == primary.as_ref() { "\tprimary" } else { "" }
                ));
            }
            event_loop.exit();
        }
        fn window_event(
            &mut self,
            _: &winit::event_loop::ActiveEventLoop,
            _: winit::window::WindowId,
            _: winit::event::WindowEvent,
        ) {
        }
    }
    winit::event_loop::EventLoop::new()?.run_app(&mut Listing)?;
    Ok(())
}

/// OSから取得した、再起動しても変わらないモニターの識別情報。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlatformMonitorIdentity {
    /// Windowsでは `DISPLAYCONFIG_TARGET_DEVICE_NAME::monitorDevicePath`。
    pub device_path: String,
    pub friendly_name: Option<String>,
}

/// モニターの照合に使う素データ。`MonitorHandle` に依存せず単体テストできる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonitorFacts {
    /// winitのモニター名。Windowsでは `\\.\DISPLAYn`。
    pub name: Option<String>,
    /// 仮想デスクトップ上の左上座標。
    pub position: (i32, i32),
    pub identity: Option<PlatformMonitorIdentity>,
}

impl MonitorFacts {
    /// 従来形式の「モニター名＋仮想座標」。
    ///
    /// モニター名だけでは同名ディスプレイを区別できないため、仮想デスクトップ上の
    /// 座標も含める。
    pub(crate) fn legacy_config_name(&self) -> String {
        let name =
            self.name.as_deref().filter(|name| !name.trim().is_empty()).unwrap_or("モニター");
        format!("{name} [{}, {}]", self.position.0, self.position.1)
    }

    /// 設定ファイルに保存する値。
    ///
    /// OSの安定した識別子を取得できた場合は「識別子＋仮想座標」、取得できない場合は
    /// 従来形式にする。座標は識別子が変わったときの照合の手掛かりとして残す。
    pub(crate) fn config_value(&self) -> String {
        match &self.identity {
            Some(identity) => {
                format!("{} [{}, {}]", identity.device_path, self.position.0, self.position.1)
            }
            None => self.legacy_config_name(),
        }
    }

    /// 設定UIに表示する名前。保存値とは分けて、人間が読める形にする。
    pub(crate) fn label(&self) -> String {
        let legacy = self.legacy_config_name();
        match self
            .identity
            .as_ref()
            .and_then(|identity| identity.friendly_name.as_deref())
            .filter(|name| !name.trim().is_empty())
        {
            Some(friendly) => format!("{friendly} ({legacy})"),
            None => legacy,
        }
    }
}

/// 設定値がどの段階でモニターに一致したか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MonitorMatch {
    /// OSの安定した識別子が一致した。
    StableId,
    /// 従来形式の「モニター名＋仮想座標」が完全に一致した。
    Legacy,
    /// 仮想座標だけが一致し、その座標のモニターが1台だけだった。
    Position,
}

impl MonitorMatch {
    /// CLIの明示指定として受け付ける一致か。座標だけの一致は推測なので除く。
    pub(crate) fn is_exact(self) -> bool {
        matches!(self, Self::StableId | Self::Legacy)
    }
}

/// 設定値を列挙済みモニターと段階的に照合する。
///
/// (a) 安定した識別子 → (b) 従来形式の完全一致 → (c) 候補が1台だけの座標一致 の順に
/// 試す。どれにも一致しなければ `None` を返し、呼び出し側がプライマリを使う。
pub(crate) fn match_configured_monitor(
    configured: &str,
    monitors: &[MonitorFacts],
) -> Option<(usize, MonitorMatch)> {
    let configured = configured.trim();
    if configured.is_empty() {
        return None;
    }
    let (head, position) = split_position_suffix(configured);

    if let Some(index) = monitors.iter().position(|monitor| {
        monitor
            .identity
            .as_ref()
            .is_some_and(|identity| identity.device_path.eq_ignore_ascii_case(head))
    }) {
        return Some((index, MonitorMatch::StableId));
    }
    if let Some(index) =
        monitors.iter().position(|monitor| monitor.legacy_config_name() == configured)
    {
        return Some((index, MonitorMatch::Legacy));
    }
    let position = position?;
    let mut candidates = monitors
        .iter()
        .enumerate()
        .filter(|(_, monitor)| monitor.position == position)
        .map(|(index, _)| index);
    match (candidates.next(), candidates.next()) {
        (Some(index), None) => Some((index, MonitorMatch::Position)),
        _ => None,
    }
}

/// 一致したモニターの保存値が設定値と異なる場合、置き換え後の値を返す。
///
/// 従来形式や座標一致で見つかった設定値を新しい識別子へ移行するために使う。
/// ただし、設定値が安定した識別子なのに一致先の識別子が取得できていない場合は
/// 従来形式へ退行してしまうため、設定値をそのまま保つ。
pub(crate) fn migrated_monitor_config_value(
    configured: &str,
    matched: &MonitorFacts,
) -> Option<String> {
    let configured = configured.trim();
    if configured.is_empty() {
        return None;
    }
    let (head, _) = split_position_suffix(configured);
    if matched.identity.is_none() && is_device_path(head) {
        return None;
    }
    let value = matched.config_value();
    (value != configured).then_some(value)
}

/// Windowsのデバイスパス（`\\?\DISPLAY#<ハードウェアID>#...`）かどうか。
fn is_device_path(value: &str) -> bool {
    value
        .strip_prefix(r"\\?\")
        .and_then(|rest| rest.split('#').nth(1))
        .is_some_and(|id| !id.is_empty())
}

/// 設定値の短い表示。接続されていない設定値をモニターなしで表示するときに使う。
///
/// Windowsのデバイスパスは `\\?\DISPLAY#<ハードウェアID>#<インスタンス>#{GUID}` の形なので、
/// ハードウェアIDと座標だけを残す。それ以外の値はそのまま返す。
pub(crate) fn monitor_config_summary(value: &str) -> String {
    let value = value.trim();
    let (head, position) = split_position_suffix(value);
    let Some(hardware_id) = head
        .strip_prefix(r"\\?\")
        .and_then(|rest| rest.split('#').nth(1))
        .filter(|id| !id.is_empty())
    else {
        return value.to_string();
    };
    match position {
        Some((x, y)) => format!("{hardware_id} [{x}, {y}]"),
        None => hardware_id.to_string(),
    }
}

/// 末尾の ` [x, y]` を座標として切り出す。
fn split_position_suffix(value: &str) -> (&str, Option<(i32, i32)>) {
    let parsed = value.strip_suffix(']').and_then(|rest| rest.rsplit_once(" [")).and_then(
        |(head, coords)| {
            let (x, y) = coords.split_once(',')?;
            Some((head, (x.trim().parse().ok()?, y.trim().parse().ok()?)))
        },
    );
    match parsed {
        Some((head, position)) => (head, Some(position)),
        None => (value, None),
    }
}

/// 同じGDIソースに複数のターゲットがある場合（複製表示）でも、毎回同じ識別子を選ぶ。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn pick_platform_identities(
    targets: HashMap<String, Vec<PlatformMonitorIdentity>>,
) -> HashMap<String, PlatformMonitorIdentity> {
    targets
        .into_iter()
        .filter_map(|(gdi_name, identities)| {
            identities
                .into_iter()
                .min_by_key(|identity| identity.device_path.to_ascii_lowercase())
                .map(|identity| (gdi_name, identity))
        })
        .collect()
}

/// 照合用の素データを付けたモニター。
#[derive(Debug, Clone)]
pub(crate) struct DescribedMonitor {
    pub handle: MonitorHandle,
    pub facts: MonitorFacts,
}

/// winitのモニター列挙に、OSの安定した識別子を対応付ける。
///
/// WindowsではGDIデバイス名（`\\.\DISPLAYn`）をキーに対応付ける。取得や対応付けに
/// 失敗したモニターは識別子なしとなり、従来形式で扱う。
pub(crate) fn describe_monitors(
    monitors: impl IntoIterator<Item = MonitorHandle>,
) -> Vec<DescribedMonitor> {
    let identities = platform_identities_by_gdi_name();
    monitors
        .into_iter()
        .map(|handle| {
            let name = handle.name();
            let position = handle.position();
            let identity = name
                .as_deref()
                .and_then(|name| identities.get(&name.to_ascii_uppercase()))
                .cloned();
            DescribedMonitor {
                facts: MonitorFacts { name, position: (position.x, position.y), identity },
                handle,
            }
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn platform_identities_by_gdi_name() -> HashMap<String, PlatformMonitorIdentity> {
    windows_display::identities_by_gdi_name()
}

#[cfg(not(target_os = "windows"))]
fn platform_identities_by_gdi_name() -> HashMap<String, PlatformMonitorIdentity> {
    HashMap::new()
}

/// 設定値から選んだモニターと、その照合結果。
#[derive(Debug, Clone)]
pub(crate) struct MonitorSelection {
    pub monitor: Option<DescribedMonitor>,
    pub matched: Option<MonitorMatch>,
}

impl MonitorSelection {
    pub(crate) fn handle(&self) -> Option<MonitorHandle> {
        self.monitor.as_ref().map(|monitor| monitor.handle.clone())
    }

    /// 設定値を新しい保存値へ置き換える必要があれば、その値を返す。
    pub(crate) fn migrated_config_value(&self, configured: &str) -> Option<String> {
        self.matched?;
        migrated_monitor_config_value(configured, &self.monitor.as_ref()?.facts)
    }

    /// 選択したモニターを、ガタつきの切り分けに使えるよう1行で記録する。
    pub(crate) fn log(&self, reason: &'static str) {
        let Some(monitor) = &self.monitor else {
            tracing::info!(reason, "no monitor available for window placement");
            return;
        };
        let refresh_hz =
            monitor.handle.refresh_rate_millihertz().map(|mhz| f64::from(mhz) / 1_000.0);
        tracing::info!(
            reason,
            monitor_id = %monitor.facts.config_value(),
            monitor_label = %monitor.facts.label(),
            refresh_hz = ?refresh_hz,
            matched = ?self.matched,
            "selected window monitor"
        );
    }
}

/// 設定されたモニターを列挙結果から選ぶ。
///
/// 設定が空、または指定されたモニターが現在接続されていない場合はプライマリ
/// モニターを使い、プライマリも取得できなければ列挙結果の先頭へフォールバックする。
/// 設定値があるのに一致しなかった場合は警告を記録する。
pub(crate) fn resolve_monitor(
    configured_name: &str,
    monitors: impl IntoIterator<Item = MonitorHandle>,
    primary: Option<MonitorHandle>,
) -> MonitorSelection {
    let monitors = describe_monitors(monitors);
    let facts = monitors.iter().map(|monitor| monitor.facts.clone()).collect::<Vec<_>>();
    if let Some((index, matched)) = match_configured_monitor(configured_name, &facts) {
        return MonitorSelection { monitor: Some(monitors[index].clone()), matched: Some(matched) };
    }
    if !configured_name.trim().is_empty() {
        tracing::warn!(
            configured_monitor = configured_name.trim(),
            available_monitors = ?facts.iter().map(MonitorFacts::config_value).collect::<Vec<_>>(),
            "configured monitor not found; using primary monitor"
        );
    }
    let monitor = primary
        .and_then(|primary| monitors.iter().find(|monitor| monitor.handle == primary).cloned())
        .or_else(|| monitors.into_iter().next());
    MonitorSelection { monitor, matched: None }
}

/// CLI・Viewerで明示されたモニターが、推測なしで現在のモニターに一致するか。
pub(crate) fn configured_monitor_exists(
    configured_name: &str,
    monitors: impl IntoIterator<Item = MonitorHandle>,
) -> bool {
    let facts =
        describe_monitors(monitors).into_iter().map(|monitor| monitor.facts).collect::<Vec<_>>();
    match_configured_monitor(configured_name, &facts).is_some_and(|(_, matched)| matched.is_exact())
}

#[cfg(target_os = "windows")]
mod windows_display {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Devices::Display::{
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
        DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes,
        QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
    };
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};

    use super::{PlatformMonitorIdentity, pick_platform_identities};

    /// 設定UIは毎フレーム列挙するため、OSへの問い合わせを短時間だけ再利用する。
    const CACHE_TTL: Duration = Duration::from_secs(2);
    type Identities = HashMap<String, PlatformMonitorIdentity>;
    static CACHE: Mutex<Option<(Instant, Identities)>> = Mutex::new(None);

    pub(super) fn identities_by_gdi_name() -> Identities {
        let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((queried_at, identities)) = cache.as_ref()
            && queried_at.elapsed() < CACHE_TTL
        {
            return identities.clone();
        }
        let identities = match query_identities() {
            Ok(identities) => identities,
            Err(status) => {
                tracing::warn!(
                    status,
                    "QueryDisplayConfig failed; identifying monitors by name and position"
                );
                HashMap::new()
            }
        };
        *cache = Some((Instant::now(), identities.clone()));
        identities
    }

    fn query_identities() -> Result<Identities, i64> {
        let mut targets = HashMap::<String, Vec<PlatformMonitorIdentity>>::new();
        for path in active_paths()? {
            let (Some(gdi_name), Some(identity)) = (source_gdi_name(&path), target_identity(&path))
            else {
                continue;
            };
            targets.entry(gdi_name.to_ascii_uppercase()).or_default().push(identity);
        }
        Ok(pick_platform_identities(targets))
    }

    /// APIによって戻り値がu32 / i32と異なるため、i64へ揃えて比べる。
    fn succeeded(status: i64) -> bool {
        status == i64::from(ERROR_SUCCESS)
    }

    fn active_paths() -> Result<Vec<DISPLAYCONFIG_PATH_INFO>, i64> {
        // 2回の呼び出しの間に構成が変わると ERROR_INSUFFICIENT_BUFFER になるため数回だけ再試行する。
        for _ in 0..4 {
            let mut path_count = 0u32;
            let mut mode_count = 0u32;
            // SAFETY: 出力先は有効なu32へのポインタ。
            let status = unsafe {
                GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
            };
            let status = i64::from(status);
            if !succeeded(status) {
                return Err(status);
            }
            // SAFETY: どちらもC構造体で、ゼロ初期化は有効な値。
            let mut paths =
                vec![unsafe { std::mem::zeroed::<DISPLAYCONFIG_PATH_INFO>() }; path_count as usize];
            let mut modes =
                vec![unsafe { std::mem::zeroed::<DISPLAYCONFIG_MODE_INFO>() }; mode_count as usize];
            // SAFETY: 配列の長さは各countと一致し、QDC_ONLY_ACTIVE_PATHSではtopology IDにnullを渡す。
            let status = unsafe {
                QueryDisplayConfig(
                    QDC_ONLY_ACTIVE_PATHS,
                    &mut path_count,
                    paths.as_mut_ptr(),
                    &mut mode_count,
                    modes.as_mut_ptr(),
                    std::ptr::null_mut(),
                )
            };
            let status = i64::from(status);
            if succeeded(status) {
                paths.truncate(path_count as usize);
                return Ok(paths);
            }
            if status != i64::from(ERROR_INSUFFICIENT_BUFFER) {
                return Err(status);
            }
        }
        Err(i64::from(ERROR_INSUFFICIENT_BUFFER))
    }

    fn source_gdi_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
        // SAFETY: C構造体で、ゼロ初期化は有効な値。
        let mut request = unsafe { std::mem::zeroed::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() };
        request.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        request.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        request.header.adapterId = path.sourceInfo.adapterId;
        request.header.id = path.sourceInfo.id;
        // SAFETY: headerのsizeは要求構造体全体の大きさと一致する。
        let status = unsafe { DisplayConfigGetDeviceInfo(&mut request.header) };
        succeeded(i64::from(status))
            .then(|| wide_to_string(&request.viewGdiDeviceName))
            .filter(|name| !name.is_empty())
    }

    fn target_identity(path: &DISPLAYCONFIG_PATH_INFO) -> Option<PlatformMonitorIdentity> {
        // SAFETY: C構造体で、ゼロ初期化は有効な値。
        let mut request = unsafe { std::mem::zeroed::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() };
        request.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
        request.header.size = size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
        request.header.adapterId = path.targetInfo.adapterId;
        request.header.id = path.targetInfo.id;
        // SAFETY: headerのsizeは要求構造体全体の大きさと一致する。
        let status = unsafe { DisplayConfigGetDeviceInfo(&mut request.header) };
        if !succeeded(i64::from(status)) {
            return None;
        }
        let device_path = wide_to_string(&request.monitorDevicePath);
        if device_path.is_empty() {
            return None;
        }
        let friendly_name = wide_to_string(&request.monitorFriendlyDeviceName);
        Some(PlatformMonitorIdentity {
            device_path,
            friendly_name: (!friendly_name.is_empty()).then_some(friendly_name),
        })
    }

    fn wide_to_string(value: &[u16]) -> String {
        let len = value.iter().position(|&unit| unit == 0).unwrap_or(value.len());
        String::from_utf16_lossy(&value[..len])
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        MonitorFacts, MonitorMatch, PlatformMonitorIdentity, match_configured_monitor,
        migrated_monitor_config_value, monitor_config_summary, pick_platform_identities,
        split_position_suffix,
    };

    const DELL_PATH: &str =
        r"\\?\DISPLAY#DEL4321#5&2a1b3c4d&0&UID4353#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
    const LG_PATH: &str =
        r"\\?\DISPLAY#GSM5B7F#5&2a1b3c4d&0&UID4357#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";

    fn legacy_monitor(name: Option<&str>, x: i32, y: i32) -> MonitorFacts {
        MonitorFacts { name: name.map(str::to_string), position: (x, y), identity: None }
    }

    fn stable_monitor(name: &str, x: i32, y: i32, path: &str, friendly: &str) -> MonitorFacts {
        MonitorFacts {
            name: Some(name.to_string()),
            position: (x, y),
            identity: Some(PlatformMonitorIdentity {
                device_path: path.to_string(),
                friendly_name: Some(friendly.to_string()),
            }),
        }
    }

    #[test]
    fn monitor_config_name_contains_name_and_virtual_position() {
        assert_eq!(
            legacy_monitor(Some("DISPLAY1"), -1920, 0).legacy_config_name(),
            "DISPLAY1 [-1920, 0]"
        );
    }

    #[test]
    fn monitor_config_name_uses_fallback_for_missing_name() {
        assert_eq!(legacy_monitor(None, 0, 0).legacy_config_name(), "モニター [0, 0]");
        assert_eq!(legacy_monitor(Some("  "), 0, 0).legacy_config_name(), "モニター [0, 0]");
    }

    #[test]
    fn monitor_config_value_prefers_stable_identifier() {
        let monitor = stable_monitor(r"\\.\DISPLAY2", 1920, 0, DELL_PATH, "DELL U2720Q");
        assert_eq!(monitor.config_value(), format!("{DELL_PATH} [1920, 0]"));
        assert_eq!(monitor.label(), r"DELL U2720Q (\\.\DISPLAY2 [1920, 0])");

        let legacy = legacy_monitor(Some("HDMI-1"), 0, 0);
        assert_eq!(legacy.config_value(), "HDMI-1 [0, 0]");
        assert_eq!(legacy.label(), "HDMI-1 [0, 0]");
    }

    #[test]
    fn match_finds_stable_identifier_after_display_renumbering_and_move() {
        // 再起動で DISPLAY 番号が入れ替わり、配置も変わった状態。
        let monitors = [
            stable_monitor(r"\\.\DISPLAY1", 0, 0, DELL_PATH, "DELL"),
            stable_monitor(r"\\.\DISPLAY2", -2560, 0, LG_PATH, "LG"),
        ];
        let configured = format!("{LG_PATH} [1920, 0]");
        assert_eq!(
            match_configured_monitor(&configured, &monitors),
            Some((1, MonitorMatch::StableId))
        );
        assert_eq!(match_configured_monitor(LG_PATH, &monitors), Some((1, MonitorMatch::StableId)));
    }

    #[test]
    fn match_accepts_exact_legacy_value() {
        let monitors = [
            stable_monitor(r"\\.\DISPLAY1", 0, 0, DELL_PATH, "DELL"),
            stable_monitor(r"\\.\DISPLAY2", 1920, 0, LG_PATH, "LG"),
        ];
        assert_eq!(
            match_configured_monitor(r"\\.\DISPLAY2 [1920, 0]", &monitors),
            Some((1, MonitorMatch::Legacy))
        );
    }

    #[test]
    fn match_falls_back_to_unique_position_for_renumbered_legacy_value() {
        let monitors = [
            stable_monitor(r"\\.\DISPLAY3", 0, 0, DELL_PATH, "DELL"),
            stable_monitor(r"\\.\DISPLAY1", 1920, 0, LG_PATH, "LG"),
        ];
        assert_eq!(
            match_configured_monitor(r"\\.\DISPLAY2 [1920, 0]", &monitors),
            Some((1, MonitorMatch::Position))
        );
        // 識別子が変わった新形式の値も、座標が一意なら同じモニターを選ぶ。
        let unknown_path = DELL_PATH.replace("UID4353", "UID9999");
        assert_eq!(
            match_configured_monitor(&format!("{unknown_path} [0, 0]"), &monitors),
            Some((0, MonitorMatch::Position))
        );
    }

    #[test]
    fn match_rejects_ambiguous_or_unknown_values() {
        // 複製表示などで同じ座標のモニターが複数ある場合は推測しない。
        let mirrored = [legacy_monitor(Some("A"), 0, 0), legacy_monitor(Some("B"), 0, 0)];
        assert_eq!(match_configured_monitor("C [0, 0]", &mirrored), None);

        let monitors = [legacy_monitor(Some("A"), 0, 0)];
        assert_eq!(match_configured_monitor("", &monitors), None);
        assert_eq!(match_configured_monitor("   ", &monitors), None);
        assert_eq!(match_configured_monitor("B [1920, 0]", &monitors), None);
        assert_eq!(match_configured_monitor("B", &monitors), None);
    }

    #[test]
    fn match_kind_exactness_excludes_position_guess() {
        assert!(MonitorMatch::StableId.is_exact());
        assert!(MonitorMatch::Legacy.is_exact());
        assert!(!MonitorMatch::Position.is_exact());
    }

    #[test]
    fn migration_replaces_legacy_value_with_stable_identifier() {
        let monitor = stable_monitor(r"\\.\DISPLAY1", 1920, 0, LG_PATH, "LG");
        assert_eq!(
            migrated_monitor_config_value(r"\\.\DISPLAY2 [1920, 0]", &monitor),
            Some(format!("{LG_PATH} [1920, 0]"))
        );
        // 識別子が一致して座標だけ変わった場合も、保存値の座標を更新する。
        assert_eq!(
            migrated_monitor_config_value(&format!("{LG_PATH} [-1920, 0]"), &monitor),
            Some(format!("{LG_PATH} [1920, 0]"))
        );
    }

    #[test]
    fn migration_keeps_stable_identifier_when_identity_is_unavailable() {
        // 識別子の取得に失敗した状態では、座標が一致しても保存済みの安定した
        // 識別子を従来形式へ退行させない。
        let legacy = legacy_monitor(Some(r"\\.\DISPLAY2"), 1920, 0);
        assert_eq!(migrated_monitor_config_value(&format!("{LG_PATH} [1920, 0]"), &legacy), None);
        // 識別子が取れている相手へは、別の安定した識別子への移行を従来どおり行う。
        let stable = stable_monitor(r"\\.\DISPLAY1", 1920, 0, DELL_PATH, "DELL");
        assert_eq!(
            migrated_monitor_config_value(&format!("{LG_PATH} [1920, 0]"), &stable),
            Some(format!("{DELL_PATH} [1920, 0]"))
        );
    }

    #[test]
    fn migration_keeps_current_or_empty_value() {
        let monitor = stable_monitor(r"\\.\DISPLAY1", 1920, 0, LG_PATH, "LG");
        assert_eq!(migrated_monitor_config_value(&format!("{LG_PATH} [1920, 0]"), &monitor), None);
        assert_eq!(migrated_monitor_config_value("", &monitor), None);
        // 識別子を取得できない環境では従来形式のまま。
        let legacy = legacy_monitor(Some("HDMI-1"), 0, 0);
        assert_eq!(migrated_monitor_config_value("HDMI-1 [0, 0]", &legacy), None);
    }

    #[test]
    fn monitor_config_summary_shortens_device_path() {
        assert_eq!(monitor_config_summary(&format!("{DELL_PATH} [1920, 0]")), "DEL4321 [1920, 0]");
        assert_eq!(monitor_config_summary(DELL_PATH), "DEL4321");
        assert_eq!(monitor_config_summary(r"\\.\DISPLAY2 [1920, 0]"), r"\\.\DISPLAY2 [1920, 0]");
    }

    #[test]
    fn split_position_suffix_parses_trailing_coordinates() {
        assert_eq!(split_position_suffix("A [-1920, 0]"), ("A", Some((-1920, 0))));
        assert_eq!(split_position_suffix("A [B] [1, 2]"), ("A [B]", Some((1, 2))));
        assert_eq!(split_position_suffix("A [x, 0]"), ("A [x, 0]", None));
        assert_eq!(split_position_suffix("A"), ("A", None));
    }

    #[test]
    fn platform_identities_pick_stable_target_for_cloned_source() {
        let identity = |path: &str| PlatformMonitorIdentity {
            device_path: path.to_string(),
            friendly_name: None,
        };
        let targets = HashMap::from([(
            r"\\.\DISPLAY1".to_string(),
            vec![identity(LG_PATH), identity(DELL_PATH)],
        )]);
        let picked = pick_platform_identities(targets);
        assert_eq!(picked[r"\\.\DISPLAY1"].device_path, DELL_PATH);
    }
}

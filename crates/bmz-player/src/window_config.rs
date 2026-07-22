use winit::monitor::MonitorHandle;

/// 設定ファイルに保存するモニター識別子を作る。
///
/// モニター名だけでは同名ディスプレイを区別できないため、仮想デスクトップ上の
/// 座標も含める。モニターが再接続された場合でも、通常は同じ識別子を復元できる。
pub(crate) fn monitor_config_name(monitor: &MonitorHandle) -> String {
    let position = monitor.position();
    monitor_config_name_from_parts(monitor.name().as_deref(), position.x, position.y)
}

fn monitor_config_name_from_parts(name: Option<&str>, x: i32, y: i32) -> String {
    let name = name.filter(|name| !name.trim().is_empty()).unwrap_or("モニター");
    format!("{name} [{x}, {y}]")
}

/// 設定されたモニターを列挙結果から選ぶ。
///
/// 設定が空、または指定されたモニターが現在接続されていない場合はプライマリ
/// モニターを使い、プライマリも取得できなければ列挙結果の先頭へフォールバックする。
pub(crate) fn select_monitor(
    configured_name: &str,
    monitors: impl IntoIterator<Item = MonitorHandle>,
    primary: Option<MonitorHandle>,
) -> Option<MonitorHandle> {
    let monitors = monitors.into_iter().collect::<Vec<_>>();
    let configured_name = configured_name.trim();

    if !configured_name.is_empty()
        && let Some(monitor) =
            monitors.iter().find(|monitor| monitor_config_name(monitor) == configured_name)
    {
        return Some(monitor.clone());
    }

    primary.or_else(|| monitors.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::monitor_config_name_from_parts;

    #[test]
    fn monitor_config_name_contains_name_and_virtual_position() {
        assert_eq!(
            monitor_config_name_from_parts(Some("DISPLAY1"), -1920, 0),
            "DISPLAY1 [-1920, 0]"
        );
    }

    #[test]
    fn monitor_config_name_uses_fallback_for_missing_name() {
        assert_eq!(monitor_config_name_from_parts(None, 0, 0), "モニター [0, 0]");
        assert_eq!(monitor_config_name_from_parts(Some("  "), 0, 0), "モニター [0, 0]");
    }
}

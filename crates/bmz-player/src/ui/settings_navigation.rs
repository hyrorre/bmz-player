use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(super) enum SettingsPage {
    #[default]
    General,
    Profile,
    Audio,
    Video,
    InputDevices,
    KeyConfig,
    Play,
    Select,
    Skin,
    Library,
    Integration,
    Import,
    Tables,
    Ir,
    Licenses,
}

impl SettingsPage {
    pub(super) fn editable_during_play(self) -> bool {
        matches!(self, Self::General | Self::Audio | Self::Video | Self::Integration)
    }

    const ALL: [Self; 15] = [
        Self::General,
        Self::Profile,
        Self::Audio,
        Self::Video,
        Self::InputDevices,
        Self::KeyConfig,
        Self::Play,
        Self::Select,
        Self::Skin,
        Self::Library,
        Self::Tables,
        Self::Ir,
        Self::Integration,
        Self::Import,
        Self::Licenses,
    ];

    fn label(self, text: Localizer) -> String {
        text.text(match self {
            Self::General => "settings-nav-general",
            Self::Profile => "settings-nav-profile",
            Self::Audio => "settings-nav-audio",
            Self::Video => "settings-nav-video",
            Self::InputDevices => "settings-input-title",
            Self::KeyConfig => "profile-key-config-title",
            Self::Play => "settings-nav-play",
            Self::Select => "settings-nav-select",
            Self::Skin => "settings-nav-skin",
            Self::Library => "settings-nav-library",
            Self::Integration => "settings-nav-integration",
            Self::Import => "settings-nav-import",
            Self::Tables => "settings-tables-title",
            Self::Ir => "profile-ir-title",
            Self::Licenses => "menu-licenses",
        })
    }

    fn subpages(self) -> &'static [&'static str] {
        match self {
            Self::Integration => {
                &["settings-nav-discord", "settings-nav-obs", "settings-screenshot-title"]
            }
            Self::Import => &["settings-score-import-title", "settings-nav-replay-import"],
            _ => &[],
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct SettingsNavigation {
    pub(super) page: SettingsPage,
    subpages: [usize; 15],
}

#[derive(Clone, Default)]
pub(super) struct SettingsFeedback {
    dirty: std::collections::HashMap<SettingsPage, (bool, bool)>,
    error: Option<String>,
    last_change: f64,
    retry_after: f64,
    errors: [Option<String>; 2],
}

impl SettingsFeedback {
    fn load(ctx: &egui::Context) -> Self {
        ctx.data_mut(|data| data.get_temp(egui::Id::new("settings_feedback")).unwrap_or_default())
    }

    fn store(&self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("settings_feedback"), self.clone()));
    }

    pub(super) fn changed(ctx: &egui::Context, app: bool, profile: bool) {
        if !app && !profile {
            return;
        }
        let mut feedback = Self::load(ctx);
        let dirty = feedback.dirty.entry(SettingsNavigation::load(ctx).page).or_default();
        dirty.0 |= app;
        dirty.1 |= profile;
        feedback.last_change = ctx.input(|input| input.time);
        feedback.store(ctx);
    }

    #[cfg(test)]
    fn has_changes(&self, page: SettingsPage) -> bool {
        self.dirty.get(&page).is_some_and(|&(app, profile)| app || profile)
    }

    fn finish_save(&mut self, app: bool, result: Result<(), String>) {
        let index = usize::from(!app);
        match result {
            Ok(()) => {
                self.errors[index] = None;
                for dirty in self.dirty.values_mut() {
                    if app {
                        dirty.0 = false;
                    } else {
                        dirty.1 = false;
                    }
                }
            }
            Err(error) => self.errors[index] = Some(error),
        }
        self.error = self.errors.iter().flatten().next().cloned();
    }

    pub(super) fn autosave(ctx: &egui::Context, closed: bool) -> (bool, bool) {
        let mut feedback = Self::load(ctx);
        let now = ctx.input(|input| input.time);
        if now < feedback.retry_after || (!closed && now - feedback.last_change < 0.5) {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
            return (false, false);
        }
        let pending = feedback.dirty.values().fold((false, false), |a, b| (a.0 || b.0, a.1 || b.1));
        if pending.0 || pending.1 {
            feedback.retry_after = now + 5.0;
            feedback.store(ctx);
        }
        pending
    }

    pub(super) fn has_pending(ctx: &egui::Context) -> bool {
        Self::load(ctx).dirty.values().any(|&(app, profile)| app || profile)
    }

    pub(super) fn profile_changed(ctx: &egui::Context, profile_id: &str) {
        let id = egui::Id::new("settings_feedback_profile");
        let changed = ctx.data_mut(|data| {
            let previous = data.get_temp::<String>(id);
            data.insert_temp(id, profile_id.to_owned());
            previous.as_deref() != Some(profile_id)
        });
        if changed {
            Self::default().store(ctx);
        }
    }
}

impl EguiLayer {
    /// 成功した保存先の保留状態を消し、失敗した保存先は再試行できるよう保持する。
    pub(crate) fn settings_save_finished(&mut self, app: bool, result: Result<(), String>) {
        let mut feedback = SettingsFeedback::load(&self.ctx);
        feedback.finish_save(app, result);
        if feedback.error.is_none() {
            feedback.retry_after = 0.0;
        }
        feedback.store(&self.ctx);
    }
}

impl SettingsNavigation {
    pub(super) fn load(ctx: &egui::Context) -> Self {
        ctx.data_mut(|data| data.get_temp(egui::Id::new("settings_navigation")).unwrap_or_default())
    }

    pub(super) fn store(&self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("settings_navigation"), self.clone()));
    }

    pub(super) fn select(ctx: &egui::Context, page: SettingsPage) {
        let mut state = Self::load(ctx);
        state.page = page;
        state.store(ctx);
    }

    fn subpage(&self) -> usize {
        self.subpages[self.page as usize]
    }

    pub(super) fn accepts_key_capture(&self) -> bool {
        self.page == SettingsPage::KeyConfig
    }
}

/// 設定セクションを選択中のページにだけ描画する。非表示のセクションは実行しない。
/// ID は翻訳された見出しに依存せず、既存の widget namespace を維持する。
pub(super) struct SettingsSection {
    page: SettingsPage,
    subpage: usize,
    title: String,
    scope: String,
    id: egui::Id,
}

impl SettingsSection {
    pub(super) fn new(page: SettingsPage, title: impl Into<String>) -> Self {
        Self {
            page,
            subpage: 0,
            title: title.into(),
            scope: String::new(),
            id: egui::Id::new(page),
        }
    }

    pub(super) fn scope(mut self, scope: String) -> Self {
        self.scope = scope;
        self
    }

    pub(super) fn id_salt(mut self, id: impl std::hash::Hash) -> Self {
        self.id = egui::Id::new(id);
        self
    }

    pub(super) fn subpage(mut self, subpage: usize) -> Self {
        self.subpage = subpage;
        self
    }

    pub(super) fn show(&self, ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
        let state = SettingsNavigation::load(ui.ctx());
        if state.page != self.page || state.subpage() != self.subpage {
            return;
        }
        ui.push_id(self.id, |ui| {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.strong(&self.title);
                ui.weak(&self.scope);
            });
            ui.separator();
            contents(ui);
            ui.add_space(12.0);
        });
    }
}

/// 自動保存の状態とナビゲーションをスクロール領域の外に配置する。
pub(super) fn build_settings_window(
    ctx: &egui::Context,
    open: &mut bool,
    profile_name: &str,
    text: Localizer,
    contents: impl FnOnce(&mut egui::Ui),
) {
    localized_sized_panel_window(
        "settings_workspace",
        tr!(text, "settings-workspace-title"),
        ctx,
        open,
        900.0,
        650.0,
        egui::pos2(220.0, 32.0),
    )
    .collapsible(false)
    .show(ctx, |ui| {
        ui.label(format!("{}: {profile_name}", tr!(text, "settings-nav-profile")));
        ui.separator();
        let mut navigation = SettingsNavigation::load(ctx);
        let body_height = (ui.available_height() - 60.0).max(40.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), body_height),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                // 狭い画面ではカテゴリ選択を本文上部の ComboBox に移す。
                let compact = ui.available_width() < 640.0;
                if !compact {
                    ui.allocate_ui_with_layout(
                        egui::vec2(170.0, body_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("settings_sidebar")
                                .max_height((body_height - 36.0).max(0.0))
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_width(160.0);
                                    for page in SettingsPage::ALL {
                                        if page == SettingsPage::Licenses {
                                            continue;
                                        }
                                        let label = page.label(text);
                                        ui.selectable_value(&mut navigation.page, page, label);
                                    }
                                });
                            ui.separator();
                            ui.selectable_value(
                                &mut navigation.page,
                                SettingsPage::Licenses,
                                SettingsPage::Licenses.label(text),
                            );
                        },
                    );
                    ui.separator();
                }
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), body_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        if compact {
                            egui::ComboBox::from_id_salt("settings_category")
                                .selected_text(navigation.page.label(text))
                                .show_ui(ui, |ui| {
                                    for page in SettingsPage::ALL {
                                        ui.selectable_value(
                                            &mut navigation.page,
                                            page,
                                            page.label(text),
                                        );
                                    }
                                });
                        } else {
                            ui.heading(navigation.page.label(text));
                        }
                        let subpages = navigation.page.subpages();
                        if !subpages.is_empty() {
                            let subpage = &mut navigation.subpages[navigation.page as usize];
                            egui::ComboBox::from_id_salt(("settings_subpage", navigation.page))
                                .selected_text(text.text(subpages[*subpage]))
                                .show_ui(ui, |ui| {
                                    for (index, key) in subpages.iter().enumerate() {
                                        ui.selectable_value(subpage, index, text.text(key));
                                    }
                                });
                        }
                        navigation.store(ctx);
                        // スクロール領域自身はページ間で同じ矩形・同じ ID を
                        // 保持する。ページごとの差分は内側の名前空間へ閉じ込め、
                        // スクロール領域そのものの ID 変更診断を発生させない。
                        let content_id =
                            ("settings_content_body", navigation.page, navigation.subpage());
                        if navigation.page == SettingsPage::Skin && ui.available_height() >= 380.0 {
                            ui.push_id(content_id, contents);
                        } else {
                            egui::ScrollArea::vertical()
                                .id_salt("settings_content")
                                .auto_shrink([false, false])
                                .max_height(ui.available_height())
                                .show(ui, |ui| {
                                    ui.push_id(content_id, contents);
                                });
                        }
                    },
                );
            },
        );
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            let feedback = SettingsFeedback::load(ctx);
            if let Some(error) = &feedback.error {
                ui.colored_label(egui::Color32::LIGHT_RED, tr!(text, "settings-save-failed"))
                    .on_hover_text(error);
            } else {
                ui.small(tr!(text, "settings-autosave-help"));
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autosave_debounces_changes_flushes_on_close_and_retries_failures() {
        let ctx = egui::Context::default();
        let frame = |time: f64, changed: (bool, bool), closed: bool| {
            let mut pending = (false, false);
            let _ = ctx.run_ui(egui::RawInput { time: Some(time), ..Default::default() }, |ui| {
                SettingsFeedback::changed(ui.ctx(), changed.0, changed.1);
                pending = SettingsFeedback::autosave(ui.ctx(), closed);
            });
            pending
        };
        assert_eq!(frame(0.0, (true, false), false), (false, false));
        assert!(SettingsFeedback::has_pending(&ctx));
        assert_eq!(frame(0.3, (false, true), false), (false, false));
        assert_eq!(frame(0.7, (false, false), false), (false, false));
        assert_eq!(frame(0.9, (false, false), false), (true, true));
        let mut feedback = SettingsFeedback::load(&ctx);
        feedback.finish_save(true, Err("Disk full".into()));
        feedback.finish_save(false, Ok(()));
        feedback.store(&ctx);
        assert_eq!(frame(1.0, (false, false), true), (false, false));
        assert_eq!(frame(6.0, (false, false), false), (true, false));
        SettingsFeedback::default().store(&ctx);
        assert_eq!(frame(7.0, (false, true), true), (false, true));
        let mut feedback = SettingsFeedback::load(&ctx);
        feedback.finish_save(false, Ok(()));
        feedback.store(&ctx);
        assert_eq!(frame(13.0, (false, false), false), (false, false));
        assert!(!SettingsFeedback::has_pending(&ctx));
    }

    #[test]
    fn failed_save_keeps_changes_when_the_other_config_saves_successfully() {
        let mut feedback = SettingsFeedback::default();
        feedback.dirty.insert(SettingsPage::Audio, (true, true));
        feedback.finish_save(true, Err("Disk full".into()));
        feedback.finish_save(false, Ok(()));
        assert_eq!(feedback.dirty[&SettingsPage::Audio], (true, false));
        assert_eq!(feedback.error.as_deref(), Some("Disk full"));
        assert!(feedback.has_changes(SettingsPage::Audio));
        feedback.finish_save(true, Ok(()));
        assert!(!feedback.has_changes(SettingsPage::Audio));
        assert!(feedback.error.is_none());
    }

    #[test]
    fn hidden_sections_do_not_run_and_subpage_selection_survives_navigation() {
        let ctx = egui::Context::default();
        let navigation = SettingsNavigation { page: SettingsPage::KeyConfig, ..Default::default() };
        navigation.store(&ctx);
        assert!(navigation.accepts_key_capture());
        SettingsNavigation::select(&ctx, SettingsPage::Audio);
        assert!(!SettingsNavigation::load(&ctx).accepts_key_capture());
        SettingsNavigation::select(&ctx, SettingsPage::KeyConfig);
        assert!(SettingsNavigation::load(&ctx).accepts_key_capture());
        let mut visible = false;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            SettingsSection::new(SettingsPage::Audio, "hidden")
                .show(ui, |_| panic!("hidden section ran"));
            SettingsSection::new(SettingsPage::InputDevices, "devices")
                .show(ui, |_| panic!("wrong subpage ran"));
            SettingsSection::new(SettingsPage::KeyConfig, "bindings").show(ui, |_| visible = true);
        });
        assert!(visible);
    }

    #[test]
    fn settings_window_keeps_autosave_status_inside_viewport_with_long_content() {
        for size in [egui::vec2(480.0, 480.0), egui::vec2(1280.0, 800.0)] {
            let ctx = egui::Context::default();
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let mut saw_save_button = false;
            for _ in 0..3 {
                let output = ctx.run_ui(
                    egui::RawInput { screen_rect: Some(rect), ..Default::default() },
                    |ui| {
                        build_settings_window(
                            ui.ctx(),
                            &mut true,
                            "Test",
                            Localizer::new(AppLocale::En),
                            |ui| {
                                for _ in 0..100 {
                                    ui.label("A setting with explanatory text");
                                }
                            },
                        );
                    },
                );
                let save = output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.job.text == "Changes are saved automatically." =>
                    {
                        Some((shape.clip_rect, text.galley.rect.translate(text.pos.to_vec2())))
                    }
                    _ => None,
                });
                // Window の初回 sizing pass には描画がないことがある。
                if let Some((clip, button)) = save {
                    saw_save_button = true;
                    assert!(rect.contains_rect(button), "save button outside viewport: {button:?}");
                    assert!(clip.contains_rect(button), "save button clipped: {button:?}");
                }
            }
            assert!(saw_save_button, "save button was not rendered");
            let window =
                ctx.memory(|memory| memory.area_rect(egui::Id::new("settings_workspace"))).unwrap();
            assert!(rect.contains_rect(window), "window outside viewport: {window:?}");
        }
    }

    #[test]
    fn license_navigation_stays_at_the_bottom_of_the_sidebar() {
        let ctx = egui::Context::default();
        let text = Localizer::new(AppLocale::En);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let mut license_rect = None;
        let mut save_rect = None;
        for _ in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput { screen_rect: Some(rect), ..Default::default() },
                |ui| {
                    build_settings_window(ui.ctx(), &mut true, "Default", text, |_| {});
                },
            );
            for shape in output.shapes {
                if let egui::Shape::Text(label) = shape.shape {
                    let rect = label.galley.rect.translate(label.pos.to_vec2());
                    if label.galley.job.text == text.text("menu-licenses") {
                        license_rect = Some(rect);
                    }
                    if label.galley.job.text == text.text("settings-autosave-help") {
                        save_rect = Some(rect);
                    }
                }
            }
        }
        let license = license_rect.expect("license navigation is visible");
        let save = save_rect.expect("save button is visible");
        assert!(license.top() > 400.0, "license should be pinned below the category list");
        assert!(license.bottom() < save.top(), "license should be above the footer");
    }

    #[test]
    fn switching_settings_pages_scopes_content_widget_ids() {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let text = Localizer::new(AppLocale::En);
        let mut open = true;
        let render = |ctx: &egui::Context, open: &mut bool| {
            ctx.run_ui(egui::RawInput { screen_rect: Some(rect), ..Default::default() }, |_| {
                build_settings_window(ctx, open, "Default", text, |ui| {
                    let rect = ui.available_rect_before_wrap();
                    let _ = ui.interact(
                        egui::Rect::from_min_size(rect.min, egui::vec2(200.0, 22.0)),
                        // 各ページの本文は同じウィジェット ID を使う。本文の親 ID
                        // がページ単位で分離されていれば、ページ切り替え時に
                        // egui の「矩形の ID 変更」診断は発生しない。
                        egui::Id::new("settings-page-first-widget"),
                        egui::Sense::click(),
                    );
                });
            })
        };
        for page in [
            SettingsPage::Library,
            SettingsPage::Tables,
            SettingsPage::Ir,
            SettingsPage::Library,
            SettingsPage::KeyConfig,
        ] {
            SettingsNavigation::select(&ctx, page);
            let output = render(&ctx, &mut open);
            let red_rects = output
                .shapes
                .iter()
                .filter(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) => {
                        rect.fill == egui::Color32::TRANSPARENT
                            && rect.stroke.color == egui::Color32::RED
                            && rect.stroke.width == 2.0
                    }
                    _ => false,
                })
                .count();
            assert_eq!(red_rects, 0, "unexpected diagnostic red rectangles on {page:?}");
        }
    }
}

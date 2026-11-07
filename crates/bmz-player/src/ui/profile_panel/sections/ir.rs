use super::*;

pub(in crate::ui::profile_panel) fn build_profile_ir_section(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
) {
    let profile = &mut *section.profile;
    let unrestricted = section.unrestricted;
    let text = section.text;
    let ir_login = &mut *section.ir_login;
    let ir_device_key = &mut *section.ir_device_key;
    let ir_manual = &mut *section.ir_manual;
    let profile_root = section.profile_root;
    SettingsSection::new(SettingsPage::Ir, tr!(text, "profile-ir-title"))
        .scope(tr!(text, "settings-scope-profile"))
        .id_salt("profile_ir")
        .show(ui, |ui| {
            if !unrestricted {
                ui.disable();
            }
            if profile.ir.normalize_builtin_providers() {
                section.save_clicked = true;
            }
            sync_ir_provider_roles(&mut profile.ir);
            let primary_options: Vec<_> = profile
                .ir
                .providers
                .iter()
                .filter_map(|provider| {
                    crate::ir::provider_key::configured_provider_key(provider).map(|provider_key| {
                        (
                            provider_key.to_string(),
                            ir_primary_provider_label(provider, provider_key),
                        )
                    })
                })
                .collect();
            let mut selected_primary = profile.ir.primary_provider.clone();
            let selected_primary_text = primary_options
                .iter()
                .find(|(provider_key, _)| provider_key == &profile.ir.primary_provider)
                .map(|(_, label)| label.clone())
                .unwrap_or_else(|| {
                    if profile.ir.primary_provider.is_empty() {
                        tr!(text, "profile-ir-unset")
                    } else {
                        profile.ir.primary_provider.clone()
                    }
                });
            egui::ComboBox::new("profile_primary_ir", tr!(text, "profile-ir-primary"))
                .selected_text(selected_primary_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut selected_primary,
                        String::new(),
                        tr!(text, "profile-ir-unset"),
                    );
                    for (provider_key, label) in &primary_options {
                        ui.selectable_value(&mut selected_primary, provider_key.clone(), label);
                    }
                });
            if selected_primary != profile.ir.primary_provider {
                profile.ir.primary_provider = selected_primary;
                sync_ir_provider_roles(&mut profile.ir);
            }
            ui.checkbox(
                &mut profile.ir.prefetch_global_ranking_on_score_submit,
                tr!(text, "profile-ir-prefetch-global"),
            );
            egui::ComboBox::new(
                "profile_ir_credential_store",
                tr!(text, "profile-ir-credential-store"),
            )
            .selected_text(match profile.ir.credential_store {
                IrCredentialStoreConfig::File => tr!(text, "profile-ir-credential-file"),
                IrCredentialStoreConfig::Os => tr!(text, "profile-ir-credential-os"),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut profile.ir.credential_store,
                    IrCredentialStoreConfig::File,
                    tr!(text, "profile-ir-credential-file"),
                );
                ui.selectable_value(
                    &mut profile.ir.credential_store,
                    IrCredentialStoreConfig::Os,
                    tr!(text, "profile-ir-credential-os"),
                );
            });
            ui.checkbox(
                &mut profile.ir.prefetch_rival_ranking_on_score_submit,
                tr!(text, "profile-ir-prefetch-rival"),
            );
            let mut remove_index = None;
            let mut logged_out_provider_key = None;
            let credential_store = profile.ir.credential_store;
            for (index, provider) in profile.ir.providers.iter_mut().enumerate() {
                ui.push_id(("ir_provider", index), |ui| {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut provider.enabled, "");
                        ui.label(tr!(text, "profile-ir-provider", "number" => index + 1));
                        if index >= BUILTIN_IR_PROVIDER_COUNT
                            && ui.button(tr!(text, "common-delete")).clicked()
                        {
                            remove_index = Some(index);
                        }
                    });
                    let provider_key = crate::ir::provider_key::configured_provider_key(provider)
                        .map(str::to_string);
                    let logged_in = provider_key.as_deref().is_some_and(|key| {
                        ir_login.is_signed_in(
                            index,
                            profile_root,
                            key,
                            &provider.base_url,
                            credential_store,
                        )
                    });
                    let endpoint_editable =
                        index >= BUILTIN_IR_PROVIDER_COUNT && !logged_in && !ir_login.busy;
                    match index {
                        0..=2 => {
                            ui.horizontal(|ui| {
                                ui.label(tr!(text, "profile-ir-provider-kind"));
                                ui.label(match index {
                                    0 => tr!(text, "profile-ir-provider-bmz"),
                                    1 => tr!(text, "profile-ir-provider-rian"),
                                    _ => tr!(text, "profile-ir-provider-bms-ir"),
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.label(tr!(text, "profile-ir-base-url"));
                                ui.add_enabled(
                                    false,
                                    egui::TextEdit::singleline(&mut provider.base_url)
                                        .desired_width(300.0),
                                );
                                ui.hyperlink_to(
                                    tr!(text, "profile-ir-open-browser"),
                                    provider.base_url.clone(),
                                );
                            });
                        }
                        _ => {
                            ui.add_enabled_ui(endpoint_editable, |ui| {
                                let mut family = ir_provider_family(&provider.provider).to_string();
                                let previous_family = family.clone();
                                egui::ComboBox::new(
                                    ("profile_ir_provider_protocol", index),
                                    tr!(text, "profile-ir-provider-protocol"),
                                )
                                .selected_text(family.clone())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut family,
                                        crate::ir::bmz_official::BMZ_IR_PROVIDER.to_string(),
                                        crate::ir::bmz_official::BMZ_IR_PROVIDER,
                                    );
                                    ui.selectable_value(
                                        &mut family,
                                        crate::ir::rian_ir::RIAN_IR_PROVIDER.to_string(),
                                        crate::ir::rian_ir::RIAN_IR_PROVIDER,
                                    );
                                });
                                if family != previous_family {
                                    provider.provider = family;
                                }
                                ir_provider_text_row(
                                    ui,
                                    &tr!(text, "profile-ir-base-url"),
                                    &mut provider.base_url,
                                );
                            });
                        }
                    }
                    if index >= BUILTIN_IR_PROVIDER_COUNT && !endpoint_editable && logged_in {
                        ui.small(tr!(text, "profile-ir-logout-to-change"));
                    }
                    let row_target = IrProviderUiTarget::new(
                        provider.provider.clone(),
                        provider.base_url.clone(),
                    );
                    let is_bms_ir = crate::ir::bms_ir::is_bms_ir_config(provider);
                    let is_rian = crate::ir::rian_ir::is_rian_ir_config(provider);
                    if index >= BUILTIN_IR_PROVIDER_COUNT {
                        let provider_key_text = provider_key
                            .clone()
                            .unwrap_or_else(|| tr!(text, "profile-ir-key-after-login"));
                        ui.horizontal(|ui| {
                            ui.label("Key");
                            ui.monospace(&provider_key_text);
                        });
                    }
                    if logged_in {
                        let display_name = if !provider.account_display_name.trim().is_empty() {
                            provider.account_display_name.trim()
                        } else if !provider.account_id.trim().is_empty() {
                            provider.account_id.trim()
                        } else {
                            provider_key.as_deref().unwrap_or_default()
                        };
                        ui.label(tr!(
                            text,
                            "profile-ir-logged-in",
                            "display_name" => display_name,
                        ));
                        if ui.button(tr!(text, "profile-ir-logout")).clicked() {
                            let result = provider_key
                                .as_deref()
                                .map(|provider_key| {
                                    crate::ir::credentials::delete_credentials(
                                        profile_root,
                                        provider_key,
                                        &provider.base_url,
                                    )
                                })
                                .transpose();
                            match result {
                                Ok(_) => {
                                    ir_login.invalidate_credential_status();
                                    provider.enabled = false;
                                    logged_out_provider_key = provider_key.clone();
                                    provider.provider_key.clear();
                                    provider.account_id.clear();
                                    provider.account_display_name.clear();
                                    provider.last_login_at = None;
                                    ir_login.message = Some(IrProviderUiMessage {
                                        provider_index: Some(index),
                                        target: row_target.clone(),
                                        ok: true,
                                        text: tr!(text, "profile-ir-logout-success"),
                                    });
                                    section.save_clicked = true;
                                }
                                Err(error) => {
                                    ir_login.message = Some(IrProviderUiMessage {
                                        provider_index: Some(index),
                                        target: row_target.clone(),
                                        ok: false,
                                        text: format!("{error:#}"),
                                    });
                                }
                            }
                        }
                    } else {
                        let form = ir_login.provider_form_mut(index);
                        ui.horizontal(|ui| {
                            ui.label(if is_rian || is_bms_ir {
                                tr!(text, "profile-ir-login-id")
                            } else {
                                tr!(text, "profile-ir-email")
                            });
                            ui.text_edit_singleline(&mut form.email);
                        });
                        ui.horizontal(|ui| {
                            ui.label(if is_bms_ir {
                                tr!(text, "profile-ir-game-token")
                            } else {
                                tr!(text, "profile-ir-password")
                            });
                            ui.add(egui::TextEdit::singleline(&mut form.password).password(true));
                        });
                        if is_bms_ir {
                            ui.small(tr!(text, "profile-ir-bms-ir-help"));
                        }
                        let credentials_ready = !form.email.is_empty() && !form.password.is_empty();
                        let can_login = !ir_login.busy
                            && normalized_ir_base_url(&provider.base_url).is_some()
                            && credentials_ready;
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    can_login,
                                    egui::Button::new(tr!(text, "profile-ir-login")),
                                )
                                .clicked()
                            {
                                ir_login.start_login(
                                    index,
                                    profile_root.to_path_buf(),
                                    provider.provider.clone(),
                                    provider.base_url.clone(),
                                );
                            }
                            let login_busy = ir_login.busy_form_index == Some(index)
                                && ir_login.busy_target.as_ref().is_some_and(|target| {
                                    target.matches(&provider.provider, &provider.base_url)
                                });
                            if login_busy {
                                ui.spinner();
                            }
                        });
                    }
                    if let Some(message) = &ir_login.message
                        && message.matches(index, &provider.provider, &provider.base_url)
                        && (!logged_in || !message.ok)
                    {
                        let color = if message.ok {
                            egui::Color32::LIGHT_GREEN
                        } else {
                            egui::Color32::LIGHT_RED
                        };
                        ui.colored_label(color, message.text.clone());
                    }
                    if logged_in {
                        ui.horizontal(|ui| {
                            let busy = ir_device_key.is_busy_for(
                                index,
                                provider_key.as_deref(),
                                &provider.provider,
                                &provider.base_url,
                            );
                            let can_rotate = !busy
                                && !provider.base_url.is_empty()
                                && provider_key.is_some()
                                && !is_rian
                                && !is_bms_ir;
                            if ui
                                .add_enabled(
                                    can_rotate,
                                    egui::Button::new(tr!(text, "profile-ir-device-key-rotate")),
                                )
                                .clicked()
                            {
                                ir_device_key.start_rotate(
                                    index,
                                    profile_root.to_path_buf(),
                                    provider.provider.clone(),
                                    provider_key.clone().unwrap_or_default(),
                                    provider.base_url.clone(),
                                );
                            }
                            if busy {
                                ui.spinner();
                            }
                        });
                        if let Some(message) = &ir_device_key.message
                            && message.matches(index, &provider.provider, &provider.base_url)
                        {
                            let color = if message.ok {
                                egui::Color32::LIGHT_GREEN
                            } else {
                                egui::Color32::LIGHT_RED
                            };
                            ui.colored_label(color, message.text.clone());
                        }
                        egui::ComboBox::new(
                            ("profile_ir_send_policy", index),
                            tr!(text, "profile-ir-send-policy"),
                        )
                        .selected_text(ir_send_policy_label(provider.send_policy))
                        .show_ui(ui, |ui| {
                            for value in [
                                IrSendPolicyConfig::UpdateScore,
                                IrSendPolicyConfig::Always,
                                IrSendPolicyConfig::CompleteSong,
                            ] {
                                ui.selectable_value(
                                    &mut provider.send_policy,
                                    value,
                                    ir_send_policy_label(value),
                                );
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(tr!(text, "profile-ir-last-login"));
                            ui.monospace(format_optional_timestamp(provider.last_login_at));
                        });
                        ui.horizontal(|ui| {
                            ui.label(tr!(text, "profile-ir-last-success"));
                            ui.monospace(format_optional_timestamp(provider.last_success_at));
                        });
                    }
                });
            }
            if logged_out_provider_key
                .as_deref()
                .is_some_and(|key| profile.ir.primary_provider == key)
            {
                profile.ir.primary_provider.clear();
                sync_ir_provider_roles(&mut profile.ir);
            }
            if let Some(index) = remove_index {
                profile.ir.providers.remove(index);
                ir_login.remove_provider_form(index);
                ir_device_key.remove_provider(index);
            }
            if ui.button(tr!(text, "profile-ir-add-provider")).clicked() {
                profile.ir.providers.push(IrProviderConfig::custom());
            }
            build_ir_bulk_operations(ui, &profile.ir, ir_manual, text);
        });
}

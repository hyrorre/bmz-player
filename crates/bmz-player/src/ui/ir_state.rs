/// プロファイル設定パネルの IR ログインフォーム状態。
///
/// ログインはネットワーク I/O なので tokio タスクで実行し、
/// 結果は channel 経由で次フレーム以降に反映する。
#[derive(Default)]
pub(super) struct IrLoginUiState {
    provider_forms: Vec<IrLoginForm>,
    pub(super) busy: bool,
    pub(super) busy_form_index: Option<usize>,
    pub(super) busy_target: Option<IrProviderUiTarget>,
    pub(super) message: Option<IrProviderUiMessage>,
    pub(super) receiver: Option<std::sync::mpsc::Receiver<Result<IrLoginOutcome, String>>>,
}

#[derive(Default)]
pub(super) struct IrLoginForm {
    pub(super) email: String,
    pub(super) password: String,
}

#[derive(Default)]
pub(super) struct ProfileManagerUiState {
    pub(super) selected_id: String,
    pub(super) available: bool,
    pub(super) busy: bool,
    pub(super) create_id: String,
    pub(super) create_display_name: String,
    pub(super) create_activate: bool,
    pub(super) copy_source_id: String,
    pub(super) copy_target_id: String,
    pub(super) copy_display_name: String,
    pub(super) copy_activate: bool,
    pub(super) message: String,
    pub(super) error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct IrProviderUiTarget {
    pub(super) provider: String,
    pub(super) base_url: String,
}

impl IrProviderUiTarget {
    pub(super) fn new(provider: String, base_url: String) -> Self {
        Self { provider, base_url }
    }

    pub(super) fn matches(&self, provider: &str, base_url: &str) -> bool {
        self.provider == provider && self.base_url == base_url
    }
}

#[derive(Debug, Clone)]
pub(super) struct IrProviderUiMessage {
    pub(super) provider_index: Option<usize>,
    pub(super) target: IrProviderUiTarget,
    pub(super) ok: bool,
    pub(super) text: String,
}

impl IrProviderUiMessage {
    pub(super) fn matches(&self, index: usize, provider: &str, base_url: &str) -> bool {
        self.provider_index == Some(index) && self.target.matches(provider, base_url)
    }
}

/// ログインタスクから UI スレッドへ返す結果。
pub(super) struct IrLoginOutcome {
    pub(super) provider: String,
    pub(super) provider_key: String,
    pub(super) base_url: String,
    pub(super) account_id: String,
    pub(super) display_name: String,
}

/// プロファイル設定パネルの IR device key 操作状態。
#[derive(Default)]
pub(super) struct IrDeviceKeyUiState {
    pub(super) busy_provider: Option<String>,
    pub(super) busy_provider_index: Option<usize>,
    pub(super) busy_target: Option<IrProviderUiTarget>,
    pub(super) message: Option<IrProviderUiMessage>,
    pub(super) receiver: Option<std::sync::mpsc::Receiver<Result<IrDeviceKeyOutcome, String>>>,
}

pub(super) struct IrDeviceKeyOutcome {
    pub(super) provider: String,
    pub(super) base_url: String,
    pub(super) public_key: String,
    pub(super) key_id: String,
}

impl IrDeviceKeyUiState {
    pub(super) fn is_busy_for(
        &self,
        index: usize,
        provider_key: Option<&str>,
        provider: &str,
        base_url: &str,
    ) -> bool {
        self.busy_provider_index == Some(index)
            && self
                .busy_provider
                .as_deref()
                .is_some_and(|busy_provider| Some(busy_provider) == provider_key)
            && self.busy_target.as_ref().is_some_and(|target| target.matches(provider, base_url))
    }

    pub(super) fn poll(&mut self, text: Localizer) {
        let Some(receiver) = &self.receiver else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(error @ std::sync::mpsc::TryRecvError::Disconnected) => {
                self.receiver = None;
                self.busy_provider = None;
                let provider_index = self.busy_provider_index.take();
                self.message = self.busy_target.take().and_then(|target| {
                    provider_index.map(|provider_index| IrProviderUiMessage {
                        provider_index: Some(provider_index),
                        target,
                        ok: false,
                        text: error.to_string(),
                    })
                });
                return;
            }
        };
        self.receiver = None;
        let target = self.busy_target.take();
        self.busy_provider = None;
        let provider_index = self.busy_provider_index.take();
        self.message = match result {
            Ok(outcome) => provider_index.map(|provider_index| IrProviderUiMessage {
                provider_index: Some(provider_index),
                target: IrProviderUiTarget::new(outcome.provider.clone(), outcome.base_url),
                ok: true,
                text: tr!(
                    text,
                    "profile-ir-device-key-rotated",
                    "provider" => outcome.provider,
                    "public_key" => short_public_key(&outcome.public_key),
                    "key_id" => outcome.key_id,
                ),
            }),
            Err(error) => target.and_then(|target| {
                provider_index.map(|provider_index| IrProviderUiMessage {
                    provider_index: Some(provider_index),
                    target,
                    ok: false,
                    text: error,
                })
            }),
        };
    }

    pub(super) fn start_rotate(
        &mut self,
        provider_index: usize,
        profile_root: std::path::PathBuf,
        provider: String,
        provider_key: String,
        base_url: String,
    ) {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.receiver = Some(receiver);
        self.busy_provider = Some(provider_key.clone());
        self.busy_provider_index = Some(provider_index);
        self.busy_target = Some(IrProviderUiTarget::new(provider.clone(), base_url.clone()));
        self.message = None;
        tokio::spawn(async move {
            let outcome = async {
                let credentials = crate::ir::sync::ensure_fresh_credentials(
                    &profile_root,
                    &provider_key,
                    &base_url,
                    now_unix_seconds(),
                )
                .await?;
                let client = crate::ir::bmz_official::BmzOfficialIrClient::new(
                    &base_url,
                    credentials.access_token,
                )?;
                let key = crate::ir::device_key::rotate_registered_device_key(
                    &profile_root,
                    &provider_key,
                    &client,
                )
                .await?;
                anyhow::Ok(IrDeviceKeyOutcome {
                    provider,
                    base_url,
                    public_key: key.public_key,
                    key_id: key.key_id.unwrap_or_default(),
                })
            }
            .await
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(outcome);
        });
    }

    pub(super) fn remove_provider(&mut self, index: usize) {
        self.busy_provider_index = shifted_index_after_removal(self.busy_provider_index, index);
        self.message = self.message.take().and_then(|mut message| {
            message.provider_index = shifted_index_after_removal(message.provider_index, index);
            message.provider_index.map(|_| message)
        });
    }
}

impl IrLoginUiState {
    pub(super) fn provider_form_mut(&mut self, index: usize) -> &mut IrLoginForm {
        if self.provider_forms.len() <= index {
            self.provider_forms.resize_with(index + 1, IrLoginForm::default);
        }
        &mut self.provider_forms[index]
    }

    pub(super) fn remove_provider_form(&mut self, index: usize) {
        if index < self.provider_forms.len() {
            self.provider_forms.remove(index);
        }
        self.busy_form_index = self.busy_form_index.and_then(|busy_index| {
            if busy_index == index {
                None
            } else if busy_index > index {
                Some(busy_index - 1)
            } else {
                Some(busy_index)
            }
        });
        self.message = self.message.take().and_then(|mut message| {
            message.provider_index = shifted_index_after_removal(message.provider_index, index);
            message.provider_index.map(|_| message)
        });
    }

    /// ログインタスクの完了を取り込み、成功時は provider 設定を更新する。
    /// profile 設定が更新された (保存が必要な) 場合に true を返す。
    pub(super) fn poll(&mut self, profile: &mut ProfileConfig, text: Localizer) -> bool {
        let Some(receiver) = &self.receiver else {
            return false;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(error @ std::sync::mpsc::TryRecvError::Disconnected) => {
                self.receiver = None;
                self.busy = false;
                let provider_index = self.busy_form_index.take();
                self.message = self.busy_target.take().and_then(|target| {
                    provider_index.map(|provider_index| IrProviderUiMessage {
                        provider_index: Some(provider_index),
                        target,
                        ok: false,
                        text: error.to_string(),
                    })
                });
                return false;
            }
        };
        self.receiver = None;
        self.busy = false;
        let form_index = self.busy_form_index.take();
        let target = self.busy_target.take();
        match result {
            Ok(outcome) => {
                if let Some(form) = form_index.and_then(|index| self.provider_forms.get_mut(index))
                {
                    form.password.clear();
                }
                self.message = form_index.map(|form_index| IrProviderUiMessage {
                    provider_index: Some(form_index),
                    target: IrProviderUiTarget::new(
                        outcome.provider.clone(),
                        outcome.base_url.clone(),
                    ),
                    ok: true,
                    text: tr!(
                        text,
                        "profile-ir-login-success",
                        "display_name" => outcome.display_name.clone(),
                    ),
                });
                if let Some(entry) = form_index
                    .and_then(|index| profile.ir.providers.get_mut(index))
                    .filter(|entry| {
                        entry.provider == outcome.provider && entry.base_url == outcome.base_url
                    })
                {
                    entry.enabled = true;
                    entry.provider_key = outcome.provider_key.clone();
                    entry.account_id = outcome.account_id;
                    entry.account_display_name = outcome.display_name;
                    entry.last_login_at = Some(now_unix_seconds());
                    if profile.ir.primary_provider.is_empty() {
                        profile.ir.primary_provider = outcome.provider_key;
                        entry.role = IrProviderRoleConfig::Primary;
                    }
                    sync_ir_provider_roles(&mut profile.ir);
                    return true;
                }
                false
            }
            Err(error) => {
                self.message = target.and_then(|target| {
                    form_index.map(|form_index| IrProviderUiMessage {
                        provider_index: Some(form_index),
                        target,
                        ok: false,
                        text: error,
                    })
                });
                false
            }
        }
    }

    /// ログインタスクを起動する。
    pub(super) fn start_login(
        &mut self,
        form_index: usize,
        profile_root: std::path::PathBuf,
        provider: String,
        base_url: String,
    ) {
        let Some(form) = self.provider_forms.get(form_index) else {
            return;
        };
        let email = form.email.clone();
        let password = form.password.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.receiver = Some(receiver);
        self.busy = true;
        self.busy_form_index = Some(form_index);
        self.busy_target = Some(IrProviderUiTarget::new(provider.clone(), base_url.clone()));
        self.message = None;
        tokio::spawn(async move {
            let outcome = async {
                let tokens = if crate::ir::bms_ir::is_bms_ir_provider(&provider) {
                    crate::ir::bms_ir::BmsIrClient::new(&base_url)?.login(&email, &password).await?
                } else if crate::ir::rian_ir::is_rian_ir_provider(&provider) {
                    crate::ir::rian_ir::RianIrClient::new(&base_url)?
                        .login(&email, &password)
                        .await?
                } else {
                    crate::ir::bmz_official::BmzOfficialIrClient::anonymous(&base_url)?
                        .login(&email, &password)
                        .await?
                };
                let provider_key = tokens.provider_key.clone();
                let display_name =
                    tokens.player.display_name.clone().unwrap_or_else(|| email.clone());
                crate::ir::credentials::save_credentials(
                    &profile_root,
                    &crate::ir::credentials::IrStoredCredentials {
                        provider: provider_key.clone(),
                        account_id: tokens.player.id.clone(),
                        display_name: display_name.clone(),
                        access_token: tokens.access_token,
                        refresh_token: tokens.refresh_token,
                        expires_at: tokens.expires_at,
                    },
                )?;
                anyhow::Ok(IrLoginOutcome {
                    provider,
                    provider_key,
                    base_url,
                    account_id: tokens.player.id,
                    display_name,
                })
            }
            .await
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(outcome);
        });
    }
}

fn shifted_index_after_removal(current: Option<usize>, removed: usize) -> Option<usize> {
    current.and_then(|current| {
        if current == removed {
            None
        } else if current > removed {
            Some(current - 1)
        } else {
            Some(current)
        }
    })
}

pub(super) fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(super) fn short_public_key(public_key: &str) -> String {
    if public_key.len() <= 16 {
        return public_key.to_string();
    }
    format!("{}…{}", &public_key[..8], &public_key[public_key.len() - 8..])
}
use super::*;

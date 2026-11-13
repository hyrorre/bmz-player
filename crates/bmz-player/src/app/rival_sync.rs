use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RianRivalSyncRequest {
    provider_key: String,
    base_url: String,
    account_id: String,
}

#[derive(Debug)]
pub(super) struct RianRivalSyncOutcome {
    provider_key: String,
    rivals: Vec<crate::ir::types::IrRivalEntry>,
}

pub(super) type RianRivalSyncWorkerResult = Result<RianRivalSyncOutcome>;

impl RianRivalSyncRequest {
    pub(super) fn from_profile(profile: &ProfileConfig) -> Option<Self> {
        let provider = crate::ir::provider_key::primary_provider_config(&profile.ir)?;
        if !crate::ir::rian_ir::is_rian_ir_config(provider)
            && !crate::ir::bms_ir::is_bms_ir_config(provider)
        {
            return None;
        }
        let provider_key = crate::ir::provider_key::configured_provider_key(provider)?;
        let account_id = provider.account_id.trim();
        if account_id.is_empty() {
            return None;
        }
        Some(Self {
            provider_key: provider_key.to_string(),
            base_url: provider.base_url.clone(),
            account_id: account_id.to_string(),
        })
    }

    async fn fetch(self, profile_root: &Path) -> Result<RianRivalSyncOutcome> {
        let rivals = if crate::ir::bms_ir::is_bms_ir_provider(&self.provider_key) {
            let credentials = crate::ir::sync::ensure_fresh_credentials(
                profile_root,
                &self.provider_key,
                &self.base_url,
                now_unix_seconds(),
            )
            .await?;
            crate::ir::bms_ir::BmsIrClient::new(&self.base_url)?
                .get_rivals(&credentials.account_id, &credentials.access_token)
                .await?
                .rivals
        } else {
            crate::ir::rian_ir::RianIrClient::new(&self.base_url)?
                .fetch_rivals(&self.account_id)
                .await?
        };
        Ok(RianRivalSyncOutcome { provider_key: self.provider_key, rivals })
    }
}

impl WinitApp {
    /// 保存済み一覧でSelectを先に表示し、初回描画後にrianIRの一覧を1回だけ更新する。
    pub(super) fn start_startup_rival_sync_after_first_frame(&mut self) {
        if !self.select_maintenance_allowed() || self.jobs.pending_rival_sync.is_some() {
            return;
        }
        let Some(request) = self.jobs.startup_rival_sync.take() else {
            return;
        };

        let (tx, rx) = mpsc::channel();
        let event_proxy = self.event_proxy.clone();
        let profile_root = self.boot.profile_paths.root_dir.clone();
        let spawn_result =
            thread::Builder::new().name("rian-rival-sync".to_string()).spawn(move || {
                let result = (|| -> RianRivalSyncWorkerResult {
                    let runtime = tokio::runtime::Runtime::new()
                        .context("failed to create rianIR rival sync runtime")?;
                    runtime.block_on(request.fetch(&profile_root))
                })();
                let _ = tx.send(result);
                let _ = event_proxy.send_event(AppUserEvent::RivalSync);
            });
        match spawn_result {
            Ok(_) => {
                self.jobs.pending_rival_sync = Some(rx);
                tracing::info!("started startup rianIR rival sync");
            }
            Err(error) => tracing::warn!(%error, "failed to spawn rianIR rival sync worker"),
        }
    }

    pub(super) fn poll_pending_rival_sync(&mut self) {
        let Some(pending) = self.jobs.pending_rival_sync.take() else {
            return;
        };
        match pending.try_recv() {
            Ok(Ok(outcome)) => {
                let previous_rival = self.boot.profile_config.rival.clone();
                let previous_updated_at = self.boot.profile_config.updated_at;
                let changed = crate::ir_cmd::sync_ir_rivals_into_profile(
                    &mut self.boot.profile_config,
                    &outcome.provider_key,
                    &outcome.rivals,
                );
                if changed {
                    self.boot.profile_config.updated_at = now_unix_seconds();
                    if let Err(error) = self.save_profile_config_now() {
                        self.boot.profile_config.rival = previous_rival;
                        self.boot.profile_config.updated_at = previous_updated_at;
                        tracing::warn!(%error, "failed to save startup rianIR rival sync");
                        return;
                    }
                }

                let target = crate::screens::select_ir::SelectRivalFetchTarget::from_profile(
                    &self.boot.profile_config,
                );
                self.select.select_ir.update_rival(target, &self.boot.profile_paths.root_dir);
                tracing::info!(
                    rivals = outcome.rivals.len(),
                    changed,
                    "startup rianIR rival sync complete"
                );
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "startup rianIR rival sync failed; using saved rivals");
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.jobs.pending_rival_sync = Some(pending);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("rianIR rival sync worker disconnected");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logged_in_rian_profile() -> ProfileConfig {
        let mut profile = ProfileConfig::new_default("test", "Test", 0);
        let provider = profile
            .ir
            .providers
            .iter_mut()
            .find(|provider| crate::ir::rian_ir::is_rian_ir_provider(&provider.provider))
            .unwrap();
        provider.provider_key = "rian-ir".to_string();
        provider.base_url = "https://rian.example.test/".to_string();
        provider.enabled = true;
        provider.account_id = "player-1".to_string();
        profile.ir.primary_provider = "rian-ir".to_string();
        profile
    }

    #[test]
    fn startup_rival_sync_uses_logged_in_primary_rian_ir() {
        let profile = logged_in_rian_profile();

        assert_eq!(
            RianRivalSyncRequest::from_profile(&profile),
            Some(RianRivalSyncRequest {
                provider_key: "rian-ir".to_string(),
                base_url: "https://rian.example.test/".to_string(),
                account_id: "player-1".to_string(),
            })
        );
    }

    #[test]
    fn startup_rival_sync_requires_a_logged_in_account() {
        let mut profile = logged_in_rian_profile();
        let provider = profile
            .ir
            .providers
            .iter_mut()
            .find(|provider| crate::ir::rian_ir::is_rian_ir_provider(&provider.provider))
            .unwrap();
        provider.account_id.clear();

        assert!(RianRivalSyncRequest::from_profile(&profile).is_none());
    }

    #[test]
    fn startup_rival_sync_ignores_non_primary_rian_ir() {
        let mut profile = logged_in_rian_profile();
        let bmz = profile
            .ir
            .providers
            .iter_mut()
            .find(|provider| !crate::ir::rian_ir::is_rian_ir_provider(&provider.provider))
            .unwrap();
        bmz.provider_key = "bmz".to_string();
        bmz.enabled = true;
        profile.ir.primary_provider = "bmz".to_string();

        assert!(RianRivalSyncRequest::from_profile(&profile).is_none());
    }
}

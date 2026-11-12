use std::fmt::Display;
use std::time::Instant;

pub(super) fn bootstrap_error_for_log(error: &anyhow::Error) -> String {
    // TOML's Display includes source lines, which may contain IR credentials.
    // Preserve the original error for the caller, but never copy config text into diagnostics.
    if let Some(parse) = error.chain().find_map(|cause| cause.downcast_ref::<toml::de::Error>()) {
        format!("configuration parse error (source omitted), byte_span={:?}", parse.span())
    } else {
        format!("{error:#}")
    }
}

pub(super) fn log_environment(paths: &crate::paths::AppPaths) {
    let (executable, executable_error) = match std::env::current_exe() {
        Ok(path) => (path.display().to_string(), None),
        Err(error) => ("<unavailable>".to_string(), Some(error.to_string())),
    };
    tracing::info!(%executable, ?executable_error,
        resource_dir = %paths.resource_dir.display(), data_dir = %paths.data_dir.display(),
        logs_dir = %paths.logs_dir.display(), "startup environment");
}

/// Inclusive wall times on one startup clock. Nested/parallel phases must not be summed.
pub(super) struct StartupPhase {
    phase: &'static str,
    clocks: Option<(Instant, Instant)>,
    finished: bool,
}

impl StartupPhase {
    pub(super) fn begin(startup: Instant, phase: &'static str) -> Self {
        Self::optional(Some(startup), phase)
    }

    /// None is used by ordinary select navigation: no clocks or diagnostic events.
    pub(super) fn optional(startup: Option<Instant>, phase: &'static str) -> Self {
        let clocks = startup.map(|startup| {
            let started = Instant::now();
            tracing::info!(
                phase,
                startup_elapsed_ms = startup.elapsed().as_millis(),
                "startup phase begin"
            );
            (startup, started)
        });
        Self { phase, clocks, finished: false }
    }

    pub(super) fn complete(mut self) {
        if let Some((startup, started)) = self.clocks {
            tracing::info!(
                phase = self.phase,
                elapsed_ms = started.elapsed().as_millis(),
                startup_elapsed_ms = startup.elapsed().as_millis(),
                "startup phase complete"
            );
        }
        self.finished = true;
    }

    /// Observe before the caller propagates an error or applies its existing fallback.
    pub(super) fn result<T, E: Display>(self, result: Result<T, E>) -> Result<T, E> {
        self.result_with_error(result, |error| format!("{error:#}"))
    }

    pub(super) fn result_with_error<T, E>(
        mut self,
        result: Result<T, E>,
        error_message: impl FnOnce(&E) -> String,
    ) -> Result<T, E> {
        match &result {
            Ok(_) => self.complete(),
            Err(error) => {
                if let Some((startup, started)) = self.clocks {
                    tracing::info!(phase = self.phase, elapsed_ms = started.elapsed().as_millis(),
                        startup_elapsed_ms = startup.elapsed().as_millis(), error = %error_message(error),
                        "startup phase failed");
                }
                self.finished = true;
            }
        }
        result
    }
}

impl Drop for StartupPhase {
    fn drop(&mut self) {
        if !self.finished
            && let Some((startup, started)) = self.clocks
        {
            // An enclosing phase was left via ? or a panic; a failing child records its error.
            tracing::info!(
                phase = self.phase,
                elapsed_ms = started.elapsed().as_millis(),
                startup_elapsed_ms = startup.elapsed().as_millis(),
                "startup phase interrupted"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn record<T>(run: impl FnOnce() -> T) -> (T, String) {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let writer = Buffer(bytes.clone());
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_target(false)
            .with_max_level(tracing::Level::INFO)
            .with_writer(move || writer.clone())
            .finish();
        let result = tracing::subscriber::with_default(subscriber, run);
        let output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        (result, output)
    }

    #[test]
    fn info_begin_precedes_work_and_complete_includes_both_durations() {
        let (_, output) = record(|| {
            let phase = StartupPhase::begin(Instant::now(), "renderer_default");
            tracing::info!("work is running");
            phase.complete();
        });
        assert!(
            output.find("startup phase begin").unwrap() < output.find("work is running").unwrap()
        );
        assert!(
            output.find("work is running").unwrap()
                < output.find("startup phase complete").unwrap()
        );
        assert!(output.contains("phase=\"renderer_default\""));
        assert!(output.contains("elapsed_ms="));
        assert!(output.contains("startup_elapsed_ms="));
    }

    #[test]
    fn failure_keeps_error_chain_and_never_reports_success() {
        let (result, output) = record(|| {
            let parent = StartupPhase::begin(Instant::now(), "parent");
            let result: anyhow::Result<()> = (|| {
                StartupPhase::begin(Instant::now(), "database_query").result(Err::<(), _>(
                    anyhow::anyhow!("original database error").context("query context"),
                ))?;
                parent.complete();
                Ok(())
            })();
            result
        });
        assert_eq!(format!("{:#}", result.unwrap_err()), "query context: original database error");
        assert!(output.contains("startup phase failed"));
        assert!(output.contains("query context: original database error"));
        assert!(output.contains("startup phase interrupted phase=\"parent\""));
        assert!(output.contains("elapsed_ms="));
        assert!(!output.contains("startup phase complete"));
    }

    #[test]
    fn configuration_parse_diagnostics_omit_source_credentials() {
        let error = toml::from_str::<toml::Value>("ir_token = \"startup-test-secret\" unexpected")
            .unwrap_err();
        let (result, output) = record(|| {
            StartupPhase::begin(Instant::now(), "bootstrap_and_boot_options").result_with_error(
                Err::<(), _>(anyhow::Error::from(error)),
                bootstrap_error_for_log,
            )
        });
        assert!(format!("{:#}", result.unwrap_err()).contains("startup-test-secret"));
        assert!(output.contains("configuration parse error"));
        assert!(!output.contains("startup-test-secret"));
    }

    fn load_root(
        boot: &crate::bootstrap::BootstrappedApp,
        startup: Option<Instant>,
        stack: &[String],
        catalog: &Result<crate::screens::select_model::VirtualFolderCatalog, String>,
    ) -> anyhow::Result<(Vec<crate::screens::select_model::SelectItem>, crate::app::SelectModeFilter)>
    {
        use crate::app::*;
        let mut cache = crate::screens::select_model::SelectCollectionCache::default();
        let context = SelectListContext {
            boot: boot.into(),
            collections: &mut cache,
            virtual_folders: catalog,
        };
        match startup {
            Some(startup) => load_items_for_stack_at_startup(
                context,
                stack,
                &[],
                SelectModeFilter::All,
                SelectDifficultyFilter::All,
                SelectSort::Title,
                startup,
            ),
            None => load_items_for_stack(
                context,
                stack,
                &[],
                SelectModeFilter::All,
                SelectDifficultyFilter::All,
                SelectSort::Title,
            ),
        }
    }

    #[test]
    fn root_diagnostics_are_limited_to_startup_and_keep_fallback_behavior() {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let boot = data.boot();
        let catalog = Err("invalid virtual folder fixture".to_string());
        let (startup, output) = record(|| load_root(&boot, Some(Instant::now()), &[], &catalog));
        assert!(startup.is_ok(), "root catalog errors must still fall back");
        for phase in [
            "select_list_sources",
            "select_root_difficulty_table_folders",
            "select_root_favorite_folders",
            "select_list_db_and_formatting",
        ] {
            assert!(output.contains(&format!("startup phase begin phase=\"{phase}\"")));
            assert!(output.contains(&format!("startup phase complete phase=\"{phase}\"")));
        }
        assert!(output.contains("startup phase failed phase=\"select_root_virtual_folders\""));
        assert!(!output.contains("startup phase complete phase=\"select_root_virtual_folders\""));
        let (normal, output) = record(|| load_root(&boot, None, &[], &catalog));
        assert_eq!(startup.unwrap().0.len(), normal.unwrap().0.len());
        assert!(!output.contains("startup phase"));
    }

    #[test]
    fn list_error_is_recorded_before_propagation() {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let boot = data.boot();
        let catalog = Err("invalid virtual folder fixture".to_string());
        let stack = vec![format!("{}missing", crate::app::VIRTUAL_FOLDER_PATH_PREFIX)];
        let (result, output) = record(|| load_root(&boot, Some(Instant::now()), &stack, &catalog));
        assert!(result.unwrap_err().to_string().contains("invalid virtual folder fixture"));
        assert!(output.contains("startup phase failed phase=\"select_list_build\""));
        assert!(!output.contains("startup phase complete phase=\"select_list_build\""));
    }
}

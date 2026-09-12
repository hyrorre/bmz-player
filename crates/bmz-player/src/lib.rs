pub mod stdio;

macro_rules! println {
    () => {
        $crate::stdio::stdout_line(format_args!(""))
    };
    ($($arg:tt)*) => {
        $crate::stdio::stdout_line(format_args!($($arg)*))
    };
}

macro_rules! eprintln {
    () => {
        $crate::stdio::stderr_line(format_args!(""))
    };
    ($($arg:tt)*) => {
        $crate::stdio::stderr_line(format_args!($($arg)*))
    };
}

pub mod app;
pub mod assist;
pub mod audio;
pub mod bootstrap;
pub mod chart_asset;
pub mod chart_preview;
pub mod cli;
pub mod config;
pub mod course;
pub mod course_cmd;
pub mod difficulty_table;
pub mod discord_presence;
pub mod gameplay_runtime;
pub mod generated_preview;
pub mod i18n;
pub mod input;
pub mod ir;
pub mod ir_cmd;
pub mod ln_policy;
pub mod logging;
pub mod obs;
pub mod paths;
pub mod practice_ui;
pub mod profile_cmd;
pub mod random_option_seed;
pub mod random_trainer;
pub mod replay_cmd;
pub mod screens;
pub mod select_options;
pub mod skin_audio;
pub(crate) mod skin_extension;
pub mod skin_loader;
pub mod song_download;
pub mod songs_cmd;
pub mod storage;
pub mod system_sound;
pub mod system_sound_manager;
pub mod table_cmd;
pub mod ui;
pub mod update;
pub mod video_bga;
pub mod video_export;
pub mod viewer_ipc;
pub mod window_config;

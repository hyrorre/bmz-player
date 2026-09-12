use std::fmt;
use std::io::{self, Write};

/// Keep the GUI subsystem (no new console on desktop launch), but make CLI
/// diagnostics visible in an existing parent's console. Run before any output.
pub fn initialize_parent_console() {
    #[cfg(windows)]
    {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(windows_console::attach_parent);
    }
}

#[cfg(windows)]
mod windows_console {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::{
            DUPLICATE_SAME_ACCESS, DuplicateHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
            SetLastError,
        },
        Storage::FileSystem::{FILE_TYPE_UNKNOWN, GetFileType},
        System::{
            Console::{
                ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE,
                STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
            },
            Threading::GetCurrentProcess,
        },
    };

    fn valid(handle: HANDLE) -> bool {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return false;
        }
        // GetFileType can legitimately return UNKNOWN; inspect last-error too.
        unsafe {
            SetLastError(0);
            GetFileType(handle) != FILE_TYPE_UNKNOWN || GetLastError() == 0
        }
    }

    pub(super) fn attach_parent() {
        // Both streams already provided (console, file, pipe or NUL): use them.
        // No process-global changes are needed for redirected CLI invocations.
        unsafe {
            if valid(GetStdHandle(STD_OUTPUT_HANDLE)) && valid(GetStdHandle(STD_ERROR_HANDLE)) {
                return;
            }
            let mut preserved = Vec::new();
            for id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                let handle = GetStdHandle(id);
                if valid(handle) {
                    let mut copy = std::ptr::null_mut();
                    let process = GetCurrentProcess();
                    if DuplicateHandle(
                        process,
                        handle,
                        process,
                        &mut copy,
                        0,
                        1,
                        DUPLICATE_SAME_ACCESS,
                    ) == 0
                    {
                        // Never risk replacing a redirected stream we cannot preserve.
                        return;
                    }
                    preserved.push((id, OwnedHandle::from_raw_handle(copy)));
                }
            }
            // Do not call AllocConsole: a desktop/editor launch stays windowless.
            if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
                return;
            }
            for (id, handle) in preserved {
                if SetStdHandle(id, handle.as_raw_handle()) != 0 {
                    // The process now owns this standard handle until termination.
                    let _ = handle.into_raw_handle();
                }
            }
        }
    }
}

pub struct SafeStderr;

pub struct SafeStderrWriter;

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SafeStderr {
    type Writer = SafeStderrWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SafeStderrWriter
    }
}

impl Write for SafeStderrWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match io::stderr().write(buf) {
            Ok(written) => Ok(written),
            Err(_) => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = io::stderr().flush();
        Ok(())
    }
}

pub fn stdout_line(args: fmt::Arguments<'_>) {
    write_line(io::stdout().lock(), args);
}

pub fn stderr_line(args: fmt::Arguments<'_>) {
    write_line(io::stderr().lock(), args);
}

fn write_line(mut writer: impl Write, args: fmt::Arguments<'_>) {
    let _ = writer.write_fmt(args);
    let _ = writer.write_all(b"\n");
}

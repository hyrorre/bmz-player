use std::path::Path;

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = match args.as_slice() {
        [option, path] if option == "--apply" => bmz_updater::process::run_request(Path::new(path))
            .map(|warning| {
                if let Some(error) = warning {
                    let message = format!("BMZ was updated. Please start BMZ manually.\n{error:#}");
                    eprintln!("{message}");
                    #[cfg(windows)]
                    show_message(
                        &message,
                        windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONWARNING,
                    );
                }
            }),
        [option, path] if option == "--recover" => bmz_updater::process::recover(Path::new(path)),
        [option, root] if option == "--prepare-installer-dll-cleanup" => {
            bmz_updater::installer::prepare_dll_cleanup(Path::new(root))
        }
        [option, root, version] if option == "--retire-installer-dlls" => {
            bmz_updater::installer::retire_dlls(Path::new(root), &version.to_string_lossy())
        }
        [option] if option == "--help" => {
            println!("bmz-updater --recover INSTALL_DIR\nInternal: --apply REQUEST.json");
            return;
        }
        _ => Err(anyhow::anyhow!("usage: bmz-updater --recover INSTALL_DIR")),
    };
    if let Err(error) = result {
        eprintln!("{error:#}");
        if args.first().is_some_and(|arg| {
            arg == "--prepare-installer-dll-cleanup" || arg == "--retire-installer-dlls"
        }) {
            std::process::exit(1);
        }
        if args.first().is_some_and(|arg| arg == "--apply") && !bmz_updater::process::committed() {
            println!("ERROR {}", format!("{error:#}").replace(['\n', '\r'], " "));
            std::process::exit(1);
        }
        #[cfg(windows)]
        show_message(
            &format!("BMZ update failed.\n{error:#}"),
            windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
        );
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn show_message(message: &str, flags: u32) {
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "BMZ Player Update".encode_utf16().chain(Some(0)).collect();
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            flags,
        );
    }
}

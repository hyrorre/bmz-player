use std::{
    fs::File,
    io,
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Component, Path, Prefix},
    ptr,
};
use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT, NtOpenFile},
    },
    Win32::{
        Foundation::{
            CloseHandle, ERROR_ACCESS_DENIED, ERROR_DELETE_PENDING, NTSTATUS, OBJ_CASE_INSENSITIVE,
            STATUS_DELETE_PENDING, STATUS_OBJECT_NAME_NOT_FOUND, STATUS_OBJECT_PATH_NOT_FOUND,
            UNICODE_STRING,
        },
        Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, SYNCHRONIZE,
        },
        System::IO::IO_STATUS_BLOCK,
    },
};

pub(crate) fn gone_or_deleting(path: &Path, error: &io::Error) -> bool {
    match error.raw_os_error().map(|code| code as u32) {
        Some(ERROR_DELETE_PENDING) => true,
        // The name can also disappear between the failed open and the native query.
        Some(ERROR_ACCESS_DENIED) => native_open_status(path).is_some_and(|status| {
            matches!(
                status,
                STATUS_DELETE_PENDING | STATUS_OBJECT_NAME_NOT_FOUND | STATUS_OBJECT_PATH_NOT_FOUND
            )
        }),
        _ => error.kind() == io::ErrorKind::NotFound,
    }
}

/// Whether `path` names a file or directory that is waiting for deletion.
pub(crate) fn delete_pending(path: &Path) -> bool {
    native_open_status(path) == Some(STATUS_DELETE_PENDING)
}

fn native_open_status(path: &Path) -> Option<NTSTATUS> {
    // Win32 maps delete-pending to access denied. Query the native status without data access.
    let parent = File::options()
        .access_mode(FILE_TRAVERSE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path.parent()?)
        .ok()?;
    let mut wide: Vec<u16> = path.file_name()?.encode_wide().collect();
    let bytes = u16::try_from(wide.len() * size_of::<u16>()).ok()?;
    let name = UNICODE_STRING { Length: bytes, MaximumLength: bytes, Buffer: wide.as_mut_ptr() };
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &raw const name,
        Attributes: OBJ_CASE_INSENSITIVE,
        ..Default::default()
    };
    let mut handle = ptr::null_mut();
    let status = unsafe {
        NtOpenFile(
            &mut handle,
            FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            &raw const attributes,
            &mut IO_STATUS_BLOCK::default(),
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        )
    };
    if !handle.is_null() {
        unsafe { CloseHandle(handle) };
    }
    Some(status)
}

pub(crate) fn verbatim_path(path: &Path) -> io::Result<Vec<u16>> {
    let path = std::path::absolute(path)?;
    let mut wide = path.as_os_str().encode_wide();
    let prefix = match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(_) => r"\\?\",
            Prefix::UNC(..) => {
                wide.next();
                wide.next();
                r"\\?\UNC\"
            }
            _ => "",
        },
        _ => "",
    };
    Ok(prefix.encode_utf16().chain(wide).chain(Some(0)).collect())
}

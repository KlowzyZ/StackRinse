//! NTFS qualification probes only. No public API or project path input.
#![cfg(windows)]

use std::{
    fs::{self, File, OpenOptions},
    io,
    mem::{offset_of, size_of},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
    ptr::null_mut,
};
use windows_sys::Win32::Storage::FileSystem::*;

fn native(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

// Concrete structure/class pairings are fixed at the call sites.
macro_rules! information {
    ($handle:expr,$class:expr,$ty:ty) => {{
        let mut info = <$ty>::default();
        // SAFETY: a live handle, writable aligned buffer, and exact structure size.
        native(unsafe {
            GetFileInformationByHandleEx(
                $handle.as_raw_handle(),
                $class,
                (&raw mut info).cast(),
                size_of::<$ty>() as u32,
            )
        })?;
        info
    }};
}

fn open(path: &Path, directory: bool, access: u32) -> io::Result<File> {
    let file = OpenOptions::new()
        .access_mode(access)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let tag = information!(file, FileAttributeTagInfo, FILE_ATTRIBUTE_TAG_INFO);
    let standard = information!(file, FileStandardInfo, FILE_STANDARD_INFO);
    if tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || standard.Directory != directory
        || (!directory && standard.NumberOfLinks != 1)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected kind, reparse point or link count",
        ));
    }
    Ok(file)
}

fn identity(file: &File) -> io::Result<(u64, [u8; 16])> {
    let info = information!(file, FileIdInfo, FILE_ID_INFO);
    Ok((info.VolumeSerialNumber, info.FileId.Identifier))
}

fn filesystem(file: &File) -> io::Result<String> {
    let mut name = [0u16; 64];
    // SAFETY: the filesystem name buffer is writable; optional outputs are null.
    native(unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    })?;
    Ok(String::from_utf16_lossy(
        &name[..name.iter().position(|&c| c == 0).unwrap_or(name.len())],
    ))
}

// Primitive probe only. The caller owns both private fixture parents and retains their handles.
fn rename_probe(file: &File, destination: &Path) -> io::Result<()> {
    let name: Vec<u16> = destination.as_os_str().encode_wide().collect();
    let bytes = name.len() * 2;
    let len = size_of::<FILE_RENAME_INFO>() + bytes;
    // usize provides native pointer alignment, including the HANDLE member.
    let mut storage = vec![0usize; len.div_ceil(size_of::<usize>())];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // SAFETY: zeroed buffer includes the full header and trailing UTF-16 name.
    // Zero ReplaceIfExists and null RootDirectory specify exclusive, absolute destination.
    unsafe {
        (*info).FileNameLength = bytes.try_into().unwrap();
        let dst = info
            .cast::<u8>()
            .add(offset_of!(FILE_RENAME_INFO, FileName))
            .cast::<u16>();
        std::ptr::copy_nonoverlapping(name.as_ptr(), dst, name.len());
        native(SetFileInformationByHandle(
            file.as_raw_handle(),
            FileRenameInfo,
            info.cast(),
            len.try_into().unwrap(),
        ))
    }
}

#[test]
fn ntfs_handle_rename_contention_and_collision() -> io::Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = open(fixture.path(), true, FILE_READ_ATTRIBUTES)?;
    let fs_name = filesystem(&root)?;
    eprintln!("Windows qualification filesystem: {fs_name}");
    assert_eq!(
        fs_name, "NTFS",
        "runner filesystem requires separate qualification"
    );
    let source = fixture.path().join("source");
    let vault = fixture.path().join("vault");
    fs::create_dir(&source)?;
    fs::create_dir(&vault)?;
    let parents = [
        open(&source, true, FILE_READ_ATTRIBUTES)?,
        open(&vault, true, FILE_READ_ATTRIBUTES)?,
    ];
    let from = source.join("sample");
    let to = vault.join("sample");
    let collision = vault.join("occupied");
    fs::write(&from, b"cache")?;
    let file = open(&from, false, FILE_READ_DATA | FILE_READ_ATTRIBUTES | DELETE)?;
    let expected = identity(&file)?;
    for parent in &parents {
        assert_eq!(identity(parent)?.0, expected.0);
    }
    std::thread::scope(|scope| {
        scope
            .spawn(|| -> io::Result<()> {
                for error in [
                    OpenOptions::new().write(true).open(&from).unwrap_err(),
                    fs::rename(&from, source.join("replacement")).unwrap_err(),
                    fs::rename(&source, fixture.path().join("relocated")).unwrap_err(),
                ] {
                    assert!(matches!(error.raw_os_error(), Some(32 | 5)), "{error}");
                }
                fs::write(&collision, b"unrelated")
            })
            .join()
            .unwrap()
    })?;
    assert!(rename_probe(&file, &collision).is_err());
    assert_eq!(fs::read(&collision)?, b"unrelated");
    rename_probe(&file, &to)?;
    assert!(!from.try_exists()?);
    assert_eq!(fs::read(&to)?, b"cache");
    assert_eq!(identity(&file)?, expected);
    rename_probe(&file, &from)?;
    drop(file);
    assert_eq!(
        identity(&open(&from, false, FILE_READ_ATTRIBUTES)?)?,
        expected
    );
    assert_eq!(fs::read(&from)?, b"cache");
    assert!(!to.try_exists()?);
    Ok(())
}

#[test]
fn rejects_existing_writer_links_and_junctions() -> io::Result<()> {
    let fixture = tempfile::tempdir()?;
    let file = fixture.path().join("data");
    fs::write(&file, b"unrelated")?;
    let writer = OpenOptions::new().write(true).open(&file)?;
    assert_eq!(
        open(&file, false, FILE_READ_ATTRIBUTES | DELETE)
            .unwrap_err()
            .raw_os_error(),
        Some(32)
    );
    drop(writer);
    fs::hard_link(&file, fixture.path().join("alias"))?;
    assert_eq!(
        open(&file, false, FILE_READ_ATTRIBUTES).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let target = fixture.path().join("target");
    fs::create_dir(&target)?;
    let result = std::process::Command::new("cmd.exe")
        .current_dir(fixture.path())
        .args(["/d", "/c", "mklink", "/J", "junction", "target"])
        .output()?;
    assert!(result.status.success(), "{result:?}");
    assert_eq!(
        open(&fixture.path().join("junction"), true, FILE_READ_ATTRIBUTES)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    match std::os::windows::fs::symlink_file(&file, fixture.path().join("symbolic")) {
        Ok(()) => assert_eq!(
            open(
                &fixture.path().join("symbolic"),
                false,
                FILE_READ_ATTRIBUTES
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::InvalidData
        ),
        Err(e) if e.raw_os_error() == Some(1314) => eprintln!("symlink privilege unavailable"),
        Err(e) => return Err(e),
    }
    assert_eq!(fs::read(file)?, b"unrelated");
    Ok(())
}

#[test]
fn replacement_between_review_and_acquisition_changes_identity() -> io::Result<()> {
    let fixture = tempfile::tempdir()?;
    let original = fixture.path().join("artifact");
    let saved = fixture.path().join("previous");
    fs::write(&original, b"same bytes")?;
    let reviewed = identity(&open(&original, false, FILE_READ_ATTRIBUTES)?)?;
    fs::rename(&original, &saved)?;
    fs::write(&original, b"same bytes")?;
    let replacement = open(&original, false, FILE_READ_ATTRIBUTES)?;
    assert_ne!(identity(&replacement)?, reviewed);
    assert_eq!(
        identity(&open(&saved, false, FILE_READ_ATTRIBUTES)?)?,
        reviewed
    );
    assert_eq!(
        rename_probe(&replacement, &fixture.path().join("other"))
            .unwrap_err()
            .raw_os_error(),
        Some(5)
    );
    assert_eq!(fs::read(&original)?, fs::read(&saved)?);
    assert!(!fixture.path().join("other").try_exists()?);
    Ok(())
}

#[test]
fn case_variant_destination_on_default_ntfs_preserves_both_files() -> io::Result<()> {
    let fixture = tempfile::tempdir()?;
    let dir = open(fixture.path(), true, FILE_READ_ATTRIBUTES)?;
    let mode = information!(dir, FileCaseSensitiveInfo, FILE_CASE_SENSITIVE_INFO);
    let original = fixture.path().join("sample");
    let occupied = fixture.path().join("occupied");
    fs::write(&original, b"cache")?;
    fs::write(&occupied, b"unrelated")?;
    let held = open(&original, false, FILE_READ_ATTRIBUTES | DELETE)?;
    if mode.Flags & 1 == 0 {
        assert!(rename_probe(&held, &fixture.path().join("OCCUPIED")).is_err());
    }
    assert!(fs::rename(&original, fixture.path().join("SAMPLE")).is_err());
    assert_eq!(fs::read(original)?, b"cache");
    assert_eq!(fs::read(occupied)?, b"unrelated");
    Ok(())
}

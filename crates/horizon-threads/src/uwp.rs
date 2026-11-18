//! The Universal Windows Platform's side of the game's folders. A UWP app, the only kind the
//! Xbox's Dev Mode runs, starts with no command line and may write only in its own data folder
//! (`LocalState`, under the package's folder in `%LOCALAPPDATA%\\Packages`): the game keeps
//! its home and its game folder there (docs/xbox-uwp.md).

use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows::Storage::ApplicationData;
use windows::Win32::System::Diagnostics::Debug::{AddVectoredExceptionHandler, EXCEPTION_POINTERS};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

/// The app's `LocalState` folder, or `None` when the process isn't a packaged app (the system
/// then has no app data to give).
pub fn local_folder() -> Option<PathBuf> {
    let path = ApplicationData::Current()
        .and_then(|data| data.LocalFolder())
        .and_then(|folder| folder.Path())
        .ok()?;
    Some(PathBuf::from(path.to_os_string()))
}

/// The file native crashes are written to, and the executable's load address.
static CRASH_FILE: OnceLock<File> = OnceLock::new();
static EXE_BASE: AtomicUsize = AtomicUsize::new(0);

/// Notes in `path` each crash Rust's panics never see: an access violation, a stack overflow,
/// an illegal instruction, a division by zero or a fail-fast, with the address in the executable
/// (`exe+0x…`, which a symbol file of the same build names). A UWP app has no console and no
/// crash dialog: without this, such a crash leaves the log cut short and nothing else. Each run
/// first adds a `start` line.
pub fn log_native_crashes(path: &Path) {
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let _ = writeln!(file, "start {since_epoch} (seconds since 1970)");
    if CRASH_FILE.set(file).is_err() {
        return;
    }
    // SAFETY: GetModuleHandleW(None) only reads the process's own module list.
    if let Ok(base) = unsafe { GetModuleHandleW(None) } {
        EXE_BASE.store(base.0 as usize, Ordering::Relaxed);
    }
    // SAFETY: the handler below only reads the exception record and writes to an open file.
    unsafe {
        AddVectoredExceptionHandler(1, Some(on_exception));
    }
}

/// Writes the crash's line, then lets the exception go on as before. Runs on the crashing
/// thread, maybe with little stack left: it formats into a buffer on the stack, no allocation.
unsafe extern "system" fn on_exception(info: *mut EXCEPTION_POINTERS) -> i32 {
    const CONTINUE_SEARCH: i32 = 0;
    // SAFETY: the system hands a valid record for the exception being dispatched.
    let Some(record) = (unsafe { info.as_ref().and_then(|info| info.ExceptionRecord.as_ref()) })
    else {
        return CONTINUE_SEARCH;
    };
    let code = record.ExceptionCode.0 as u32;
    let what = match code {
        0xC000_0005 => "access violation",
        0xC000_00FD => "stack overflow",
        0xC000_001D => "illegal instruction",
        0xC000_0094 => "integer division by zero",
        0xC000_0409 => "fail-fast",
        _ => return CONTINUE_SEARCH,
    };
    let address = record.ExceptionAddress as usize;
    let base = EXE_BASE.load(Ordering::Relaxed);
    let mut line = StackLine::default();
    let _ = writeln!(
        line,
        "native crash: {what} (0x{code:08x}) at exe+0x{:x} (0x{address:x})",
        address.wrapping_sub(base),
    );
    if let Some(mut file) = CRASH_FILE.get() {
        let _ = file.write_all(line.as_bytes());
    }
    CONTINUE_SEARCH
}

/// A line formatted on the stack, cut at 256 bytes.
struct StackLine {
    bytes: [u8; 256],
    len: usize,
}

impl Default for StackLine {
    fn default() -> Self {
        Self {
            bytes: [0; 256],
            len: 0,
        }
    }
}

impl StackLine {
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl std::fmt::Write for StackLine {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let take = text.len().min(self.bytes.len() - self.len);
        self.bytes[self.len..self.len + take].copy_from_slice(&text.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

/// The `KEY=VALUE` lines [`apply_env_file`] set, for the log once it is open.
static ENV_FILE_SET: OnceLock<Vec<String>> = OnceLock::new();

/// Sets the environment variables `path` lists, one `KEY=VALUE` a line (`#` starts a comment):
/// a UWP app gets neither a command line nor an environment of the player's, and these are how
/// the game's switches are given to it (`HORIZON_DROP_FEATURES`...). Call it first thing in
/// `main`, before any other thread: changing the environment is only sound then.
pub fn apply_env_file(path: &Path) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let mut set = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if key.is_empty() || key.contains('\0') || value.contains('\0') {
            continue;
        }
        // SAFETY: called before the process starts any other thread (see above), so nothing
        // reads the environment meanwhile.
        unsafe { std::env::set_var(key, value) };
        set.push(format!("{key}={value}"));
    }
    let _ = ENV_FILE_SET.set(set);
}

/// What [`apply_env_file`] set.
pub fn env_file_set() -> &'static [String] {
    ENV_FILE_SET.get().map_or(&[], Vec::as_slice)
}

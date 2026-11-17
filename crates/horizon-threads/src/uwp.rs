//! The Universal Windows Platform's side of the game's folders. A UWP app, the only kind the
//! Xbox's Dev Mode runs, starts with no command line and may write only in its own data folder
//! (`LocalState`, under the package's folder in `%LOCALAPPDATA%\\Packages`): the game keeps
//! its home and its game folder there (docs/xbox-uwp.md).

use std::path::PathBuf;

use windows::Storage::ApplicationData;

/// The app's `LocalState` folder, or `None` when the process isn't a packaged app (the system
/// then has no app data to give).
pub fn local_folder() -> Option<PathBuf> {
    let path = ApplicationData::Current()
        .and_then(|data| data.LocalFolder())
        .and_then(|folder| folder.Path())
        .ok()?;
    Some(PathBuf::from(path.to_os_string()))
}

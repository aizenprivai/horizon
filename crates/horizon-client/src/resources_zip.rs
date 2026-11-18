//! The prepared game files as one zip, unpacked into place: how the Xbox's UWP build gets them
//! (docs/xbox-uwp.md). Its player uploads `resources.zip` to the app's `LocalState` folder with
//! the console's Device Portal, which takes one file at a time; the no-resources screen unpacks
//! it next to itself, into `resources`, with the window already up (a UWP app that takes too
//! long to show its window at launch is ended by the system).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The zip the player uploads.
pub const ZIP_NAME: &str = "resources.zip";
/// The folder it is unpacked to, beside it.
pub const FOLDER_NAME: &str = "resources";
/// The file that marks a folder of prepared game files (horizon-prepare's manifest).
const MANIFEST: &str = "horizon-resources.json";

/// How far an [`unpack`] is: files written and files to write, read from another thread.
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
}

impl Progress {
    pub fn get(&self) -> (usize, usize) {
        (
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }
}

/// Unpacks the prepared game files in `zip` to `dest`. They are the folder holding
/// `horizon-resources.json`: the zip's root, or the shallowest folder in it that has one (a
/// zip of the `26.3` folder itself). Everything goes to `<dest>.partial` first and replaces
/// `dest` only once all of it is written, so a run stopped halfway leaves no half folder to
/// start from. A zip with an entry that would land outside its folder is refused whole.
pub fn unpack(zip: &Path, dest: &Path, progress: &Progress) -> Result<(), String> {
    let file = fs::File::open(zip).map_err(|e| format!("{}: {e}", zip.display()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", zip.display()))?;
    let mut names = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = entry.enclosed_name() else {
            return Err(format!("{}: unsafe path {:?}", zip.display(), entry.name()));
        };
        names.push((name, entry.is_dir()));
    }
    let root = names
        .iter()
        .filter(|(name, dir)| !dir && name.file_name() == Some(MANIFEST.as_ref()))
        .filter_map(|(name, _)| name.parent())
        .min_by_key(|parent| parent.components().count())
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("{}: no {MANIFEST} in it", zip.display()))?;
    let files = names
        .iter()
        .filter(|(name, dir)| !dir && name.starts_with(&root))
        .count();
    progress.total.store(files, Ordering::Relaxed);
    progress.done.store(0, Ordering::Relaxed);

    let partial = partial_of(dest);
    remove_dir(&partial)?;
    fs::create_dir_all(&partial).map_err(|e| format!("{}: {e}", partial.display()))?;
    for (i, (name, dir)) in names.iter().enumerate() {
        let Ok(relative) = name.strip_prefix(&root) else {
            continue;
        };
        let out = partial.join(relative);
        if *dir {
            fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let mut target = fs::File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        io::copy(&mut entry, &mut target).map_err(|e| format!("{}: {e}", out.display()))?;
        progress.done.fetch_add(1, Ordering::Relaxed);
    }
    remove_dir(dest)?;
    fs::rename(&partial, dest).map_err(|e| format!("{}: {e}", dest.display()))
}

/// `<dest>.partial`, beside `dest`.
fn partial_of(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    dest.with_file_name(name)
}

fn remove_dir(dir: &Path) -> Result<(), String> {
    match fs::remove_dir_all(dir) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(format!("{}: {e}", dir.display())),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A fresh folder of its own under the system's temporary folder.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "horizon-resources-zip-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_zip(path: &Path, entries: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, text) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    /// A zip of the version folder itself unpacks its contents, replacing an older folder.
    #[test]
    fn unpacks_the_folder_holding_the_manifest() {
        let dir = scratch("nested");
        let zip = dir.join(ZIP_NAME);
        write_zip(
            &zip,
            &[
                ("26.3/horizon-resources.json", "{}"),
                ("26.3/assets/lang.json", "lang"),
                ("notes.txt", "outside the files"),
            ],
        );
        let dest = dir.join(FOLDER_NAME);
        fs::create_dir_all(dest.join("stale")).unwrap();
        let progress = Progress::default();
        unpack(&zip, &dest, &progress).unwrap();
        assert_eq!(
            fs::read_to_string(dest.join("horizon-resources.json")).unwrap(),
            "{}"
        );
        assert_eq!(
            fs::read_to_string(dest.join("assets/lang.json")).unwrap(),
            "lang"
        );
        assert!(!dest.join("stale").exists());
        assert!(!dest.join("notes.txt").exists());
        assert!(!partial_of(&dest).exists());
        assert_eq!(progress.get(), (2, 2));
        fs::remove_dir_all(dir).unwrap();
    }

    /// A zip without prepared files, or with a path out of its folder, changes nothing.
    #[test]
    fn refuses_a_zip_without_the_manifest_or_with_an_escaping_path() {
        let dir = scratch("refused");
        let dest = dir.join(FOLDER_NAME);
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("horizon-resources.json"), "old").unwrap();
        let cases: [&[(&str, &str)]; 2] = [
            &[("assets/lang.json", "lang")],
            &[("horizon-resources.json", "{}"), ("../evil.txt", "evil")],
        ];
        for entries in cases {
            let zip = dir.join(ZIP_NAME);
            write_zip(&zip, entries);
            assert!(unpack(&zip, &dest, &Progress::default()).is_err());
            assert_eq!(
                fs::read_to_string(dest.join("horizon-resources.json")).unwrap(),
                "old"
            );
            assert!(!dir.join("evil.txt").exists());
        }
        fs::remove_dir_all(dir).unwrap();
    }
}

//! Parallel, filtered zip extraction and all-or-nothing installs (DESIGN_SPEC §3).

use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub files: usize,
    pub bytes: u64,
    pub skipped: usize,
}

/// Extracts the entries of `zip` for which `keep` returns true into `dest`.
///
/// `keep` sees the entry path with `/` separators and, if `strip_top` is set,
/// without the archive's single top-level folder (`mariadb-11.4.13-winx64/`).
/// Entries that would escape `dest` (zip-slip) are rejected.
pub fn extract_zip(
    zip: &Path,
    dest: &Path,
    strip_top: bool,
    keep: &(dyn Fn(&str) -> bool + Sync),
) -> io::Result<Stats> {
    let mut archive = open(zip)?;
    let mut jobs: Vec<(usize, PathBuf, u64)> = Vec::new();
    let mut skipped = 0;

    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i).map_err(zip_err)?;
        if entry.is_dir() {
            continue;
        }
        let Some(name) = entry.enclosed_name() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsafe path in archive: {}", entry.name()),
            ));
        };
        let rel: PathBuf = if strip_top {
            name.components().skip(1).collect()
        } else {
            name
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str.is_empty() || !keep(&rel_str) {
            skipped += 1;
            continue;
        }
        jobs.push((i, rel, entry.size()));
    }

    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let jobs = &jobs;
                scope.spawn(move || -> io::Result<()> {
                    // Each worker needs its own file handle; ZipArchive is not shareable.
                    let mut archive = open(zip)?;
                    for (i, rel, _) in jobs.iter().skip(t).step_by(threads) {
                        let target = dest.join(rel);
                        if let Some(parent) = target.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        let mut entry = archive.by_index(*i).map_err(zip_err)?;
                        let mut out = BufWriter::with_capacity(256 * 1024, File::create(&target)?);
                        io::copy(&mut entry, &mut out)?;
                    }
                    Ok(())
                })
            })
            .collect();
        handles.into_iter().try_for_each(|h| {
            h.join()
                .unwrap_or_else(|_| Err(io::Error::other("extraction thread panicked")))
        })
    })?;

    Ok(Stats {
        files: jobs.len(),
        bytes: jobs.iter().map(|j| j.2).sum(),
        skipped,
    })
}

/// Builds `final_dir` in a sibling temp directory and renames it into place, so
/// an interrupted install never leaves a half-populated version behind.
pub fn install_atomically(final_dir: &Path, build: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
    let name = final_dir
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "install dir has no name"))?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(".tmp");
    let tmp = final_dir.with_file_name(tmp_name);

    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir_all(&tmp)?;
    match build(&tmp) {
        Ok(()) => std::fs::rename(&tmp, final_dir),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(e)
        }
    }
}

fn open(zip: &Path) -> io::Result<ZipArchive<BufReader<File>>> {
    ZipArchive::new(BufReader::new(File::open(zip)?)).map_err(zip_err)
}

fn zip_err(e: zip::result::ZipError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("phpev-extract-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_zip(path: &Path, entries: &[(&str, &str)]) {
        let mut w = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, body) in entries {
            w.start_file(*name, SimpleFileOptions::default()).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn strips_top_folder_and_filters() {
        let dir = temp("filter");
        let zip = dir.join("a.zip");
        make_zip(
            &zip,
            &[
                ("pkg-1.0/bin/keep.exe", "x"),
                ("pkg-1.0/bin/drop.pdb", "y"),
                ("pkg-1.0/share/a/b.txt", "zz"),
            ],
        );
        let out = dir.join("out");
        let stats = extract_zip(&zip, &out, true, &|p| !p.ends_with(".pdb")).unwrap();
        assert_eq!(
            stats,
            Stats {
                files: 2,
                bytes: 3,
                skipped: 1
            }
        );
        assert_eq!(std::fs::read_to_string(out.join(r"share\a\b.txt")).unwrap(), "zz");
        assert!(!out.join(r"bin\drop.pdb").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_zip_slip() {
        let dir = temp("slip");
        let zip = dir.join("evil.zip");
        make_zip(&zip, &[("../evil.txt", "x")]);
        assert!(extract_zip(&zip, &dir.join("out"), false, &|_| true).is_err());
        assert!(!dir.join("evil.txt").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn failed_install_leaves_nothing_behind() {
        let dir = temp("atomic");
        let target = dir.join("8.4.27");
        let err = install_atomically(&target, |tmp| {
            std::fs::write(tmp.join("php.exe"), "x")?;
            Err(io::Error::other("boom"))
        });
        assert!(err.is_err());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);

        install_atomically(&target, |tmp| std::fs::write(tmp.join("php.exe"), "x")).unwrap();
        assert!(target.join("php.exe").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

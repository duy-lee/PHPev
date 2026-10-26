//! Code editors installed on this computer, for "Open in editor".

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Editor {
    /// `vscode`, `cursor`, `phpstorm`, `sublime`, `notepadpp`.
    pub id: String,
    pub name: String,
    pub exe: PathBuf,
}

struct Known {
    id: &'static str,
    name: &'static str,
    /// Install locations; the first one names the program file.
    places: &'static [(Base, &'static str)],
    /// Names on PATH; a `.cmd` launcher leads to the program next to it.
    on_path: &'static [&'static str],
}

const KNOWN: &[Known] = &[
    Known {
        id: "vscode",
        name: "Visual Studio Code",
        places: &[
            (Base::LocalPrograms, r"Microsoft VS Code\Code.exe"),
            (Base::ProgramFiles, r"Microsoft VS Code\Code.exe"),
        ],
        on_path: &["code.cmd"],
    },
    Known {
        id: "cursor",
        name: "Cursor",
        places: &[(Base::LocalPrograms, r"cursor\Cursor.exe")],
        on_path: &["cursor.cmd"],
    },
    Known {
        id: "phpstorm",
        name: "PhpStorm",
        places: &[
            // JetBrains Toolbox 2.x
            (Base::LocalPrograms, r"PhpStorm\bin\phpstorm64.exe"),
            (Base::JetBrains, "PhpStorm"),
        ],
        on_path: &["phpstorm64.exe"],
    },
    Known {
        id: "sublime",
        name: "Sublime Text",
        places: &[
            (Base::ProgramFiles, r"Sublime Text\sublime_text.exe"),
            (Base::ProgramFiles, r"Sublime Text 3\sublime_text.exe"),
        ],
        on_path: &["subl.exe"],
    },
    Known {
        id: "notepadpp",
        name: "Notepad++",
        places: &[(Base::ProgramFiles, r"Notepad++\notepad++.exe")],
        on_path: &["notepad++.exe"],
    },
];

#[derive(Clone, Copy)]
enum Base {
    /// `%LOCALAPPDATA%\Programs` (per-user installs)
    LocalPrograms,
    /// `%ProgramFiles%` and `%ProgramFiles(x86)%`
    ProgramFiles,
    /// `%ProgramFiles%\JetBrains\<name> <version>\bin\<name>64.exe`, newest first
    JetBrains,
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

fn candidates(base: Base, rel: &str) -> Vec<PathBuf> {
    match base {
        Base::LocalPrograms => env_dir("LOCALAPPDATA")
            .map(|d| d.join("Programs").join(rel))
            .into_iter()
            .collect(),
        Base::ProgramFiles => ["ProgramFiles", "ProgramFiles(x86)"]
            .iter()
            .filter_map(|v| env_dir(v))
            .map(|d| d.join(rel))
            .collect(),
        Base::JetBrains => {
            let Some(root) = env_dir("ProgramFiles").map(|d| d.join("JetBrains")) else {
                return Vec::new();
            };
            let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(rel)))
                .collect();
            dirs.sort();
            dirs.reverse();
            dirs.into_iter()
                .map(|d| d.join("bin").join(format!("{}64.exe", rel.to_lowercase())))
                .collect()
        }
    }
}

fn on_path(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(program))
        .find(|p| p.is_file())
}

/// `code.cmd` on PATH → the `Code.exe` a few folders above it.
fn exe_near(script: &Path, exe_name: &str) -> Option<PathBuf> {
    script
        .ancestors()
        .skip(1)
        .take(4)
        .map(|d| d.join(exe_name))
        .find(|p| p.is_file())
}

fn find(known: &Known) -> Option<PathBuf> {
    let exe_name = Path::new(known.places.first()?.1)
        .file_name()?
        .to_string_lossy()
        .into_owned();
    known
        .places
        .iter()
        .flat_map(|(base, rel)| candidates(*base, rel))
        .chain(known.on_path.iter().filter_map(|p| on_path(p)).filter_map(|p| {
            if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
                Some(p)
            } else {
                exe_near(&p, &exe_name)
            }
        }))
        .find(|p| p.is_file())
}

/// The editors found, in a fixed order.
pub fn installed() -> Vec<Editor> {
    KNOWN
        .iter()
        .filter_map(|k| {
            Some(Editor {
                exe: find(k)?,
                id: k.id.into(),
                name: k.name.into(),
            })
        })
        .collect()
}

/// Names of every editor PHPev looks for, to show when none was found.
pub fn supported() -> Vec<&'static str> {
    KNOWN.iter().map(|k| k.name).collect()
}

/// Opens folder `dir` in `editor`.
pub fn open(editor: &Editor, dir: &Path) -> io::Result<()> {
    crate::launch::open_with(&editor.exe, dir)
}

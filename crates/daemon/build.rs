//! Embeds the PHPev icon (resource id 1) and version information, so Explorer,
//! Task Manager and the tray show PHPev instead of a generic program.
//! Needs rc.exe (MSVC, Windows SDK) or windres (GNU, MinGW-w64).
//!
//! Also generates the table of dashboard files the daemon serves
//! (`dashboard_assets.rs`), so a new file under `dashboard/` needs no code.

use std::path::{Path, PathBuf};

/// Files served from outside `dashboard/`: URL path and source, relative to the crate.
const EXTRA_ASSETS: &[(&str, &str)] = &[
    ("/favicon.ico", "../../assets/web/favicon.ico"),
    ("/favicon.svg", "../../assets/web/favicon.svg"),
    ("/logo-light.svg", "../../assets/phpev-logo-light.svg"),
    ("/logo-dark.svg", "../../assets/phpev-logo-dark.svg"),
];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    embed_dashboard(&manifest);
    let icon = manifest.join("../../assets/phpev.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let numbers: Vec<&str> = version.split(['.', '-']).take(3).collect();
    let rc = format!(
        r#"#include <winver.h>
1 ICON "{icon}"
VS_VERSION_INFO VERSIONINFO
FILEVERSION {n},0
PRODUCTVERSION {n},0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "PHPev background service"
      VALUE "ProductName", "PHPev"
      VALUE "FileVersion", "{version}"
      VALUE "ProductVersion", "{version}"
      VALUE "OriginalFilename", "phpev-daemon.exe"
      VALUE "LegalCopyright", "MIT OR Apache-2.0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#,
        icon = icon.display().to_string().replace('\\', "/"),
        n = numbers.join(","),
    );
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("phpev.rc");
    std::fs::write(&out, rc).unwrap();
    embed_resource::compile(&out, embed_resource::NONE)
        .manifest_required()
        .unwrap();
}

/// Writes `lookup(path) -> Option<(bytes, content type)>` over every file in
/// `dashboard/` (served at its path, `index.html` also at `/`) and EXTRA_ASSETS.
fn embed_dashboard(manifest: &Path) {
    let dir = manifest.join("dashboard");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files = Vec::new();
    collect(&dir, &dir, &mut files);
    for (url, source) in EXTRA_ASSETS {
        let path = manifest.join(source);
        println!("cargo:rerun-if-changed={}", path.display());
        files.push((url.to_string(), path));
    }
    files.sort();

    let mut code = String::from(
        "/// Embedded dashboard file at `path`: its bytes and content type.\n\
         pub fn lookup(path: &str) -> Option<(&'static [u8], &'static str)> {\n    Some(match path {\n",
    );
    for (url, path) in &files {
        let mime = content_type(path)
            .unwrap_or_else(|| panic!("no content type for {}; add its extension to build.rs", path.display()));
        let pattern = if url == "/index.html" {
            "\"/\" | \"/index.html\"".to_owned()
        } else {
            format!("{url:?}")
        };
        code.push_str(&format!(
            "        {pattern} => (include_bytes!({:?}), {mime:?}),\n",
            path.display().to_string()
        ));
    }
    code.push_str("        _ => return None,\n    })\n}\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("dashboard_assets.rs");
    std::fs::write(out, code).unwrap();
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(root, &path, files);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
            let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            files.push((format!("/{rel}"), path));
        }
    }
}

fn content_type(path: &Path) -> Option<&'static str> {
    Some(match path.extension()?.to_str()? {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "png" => "image/png",
        "txt" => "text/plain; charset=utf-8",
        _ => return None,
    })
}

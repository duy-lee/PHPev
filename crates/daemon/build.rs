//! Embeds the PHPev icon (resource id 1), the tray icon (id 2, transparent so it
//! stays visible on a dark taskbar) and version information, so Explorer, Task
//! Manager and the tray show PHPev instead of a generic program.
//! Needs rc.exe (MSVC, Windows SDK) or windres (GNU, MinGW-w64).

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let icon = manifest.join("../../assets/phpev.ico");
    let tray = manifest.join("../../assets/phpev-tray.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed={}", tray.display());
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let numbers: Vec<&str> = version.split(['.', '-']).take(3).collect();
    let rc = format!(
        r#"#include <winver.h>
1 ICON "{icon}"
2 ICON "{tray}"
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
        tray = tray.display().to_string().replace('\\', "/"),
        n = numbers.join(","),
    );
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("phpev.rc");
    std::fs::write(&out, rc).unwrap();
    embed_resource::compile(&out, embed_resource::NONE)
        .manifest_required()
        .unwrap();
}

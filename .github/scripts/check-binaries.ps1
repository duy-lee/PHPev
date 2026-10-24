# Checks the release executables:
# - no dependency on the Visual C++ runtime DLLs (the C runtime is linked
#   statically, so PHPev runs on a clean Windows machine);
# - the icon and version resources are embedded;
# - size budget (warning only).
param([Parameter(Mandatory)] [string] $Dir)
$ErrorActionPreference = 'Stop'

$exes = 'phpev.exe', 'phpev-daemon.exe', 'phpev-elevate.exe', 'phpev-shim.exe'
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$dumpbin = & $vswhere -latest -products * -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe' | Select-Object -First 1
if (-not $dumpbin) { throw 'dumpbin.exe not found' }

$failed = $false
foreach ($name in $exes) {
    $exe = Join-Path $Dir $name
    if (-not (Test-Path $exe)) { Write-Host "::error::$name is missing"; $failed = $true; continue }

    $deps = & $dumpbin /nologo /dependents $exe | Where-Object { $_ -match '\.dll\s*$' } | ForEach-Object { $_.Trim() }
    $crt = $deps | Where-Object { $_ -match '^(vcruntime|msvcp|ucrtbase|api-ms-win-crt-)' }
    if ($crt) { Write-Host "::error::$name depends on the C runtime: $($crt -join ', ')"; $failed = $true }

    $size = (Get-Item $exe).Length
    Write-Host ("{0,-20} {1,8:N0} KB  imports: {2}" -f $name, ($size / 1KB), ($deps -join ', '))
    if ($name -eq 'phpev-daemon.exe' -and $size -gt 8MB) {
        Write-Host "::warning::phpev-daemon.exe is larger than 8 MB ($([math]::Round($size / 1MB, 1)) MB)"
    }
}

foreach ($name in 'phpev.exe', 'phpev-daemon.exe') {
    $info = (Get-Item (Join-Path $Dir $name)).VersionInfo
    if ($info.ProductName -ne 'PHPev') { Write-Host "::error::$name has no PHPev version resource"; $failed = $true }
    $rsrc = & $dumpbin /nologo /headers (Join-Path $Dir $name) | Select-String '\.rsrc'
    if (-not $rsrc) { Write-Host "::error::$name has no resource section (icon)"; $failed = $true }
}

if ($failed) { exit 1 }

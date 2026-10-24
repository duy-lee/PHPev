# Builds dist/phpev-<version>-setup.exe from target/release with Inno Setup,
# installing it through Chocolatey when the runner does not have it.
param(
    [Parameter(Mandatory)] [string] $Version
)
$ErrorActionPreference = 'Stop'

function Find-Iscc {
    $dirs = 'Inno Setup 7', 'Inno Setup 6'
    foreach ($root in $env:ProgramFiles, ${env:ProgramFiles(x86)}) {
        foreach ($dir in $dirs) {
            $exe = Join-Path $root "$dir\ISCC.exe"
            if (Test-Path $exe) { return $exe }
        }
    }
}

$iscc = Find-Iscc
if (-not $iscc) {
    choco install innosetup --no-progress -y | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "installing Inno Setup failed" }
    $iscc = Find-Iscc
    if (-not $iscc) { throw "ISCC.exe not found after installing Inno Setup" }
}

& $iscc /Q "/DAppVersion=$Version" installer/phpev.iss
if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }
Get-Item "dist/phpev-$Version-setup.exe" | Select-Object Name, Length | Out-Host

# Writes the GitHub Release notes for a version:
# - a short introduction;
# - its section of CHANGELOG.md (Install, Added, Fixed, ...);
# - "Changelog": the commits since the previous tag, grouped by their
#   `type:` prefix (feat → Features, fix → Bug Fixes, the rest → Other).
#   Merge and `release:` commits are left out.
param(
    [Parameter(Mandatory)] [string] $Version,
    [Parameter(Mandatory)] [string] $Tag,
    [string] $Out = 'dist/notes.md'
)
$ErrorActionPreference = 'Stop'

$lines = Get-Content CHANGELOG.md
$start = ($lines | Select-String -Pattern "^## \[$([regex]::Escape($Version))\]").LineNumber
if (-not $start) { throw "CHANGELOG.md has no section for $Version" }
$rest = $lines[$start..($lines.Length - 1)]
$end = ($rest | Select-String -Pattern '^## \[' | Select-Object -First 1).LineNumber
$section = if ($end) { $rest[0..($end - 2)] } else { $rest }
# The link definitions at the end of the file belong to no section.
$section = $section | Where-Object { $_ -notmatch '^\[[^\]]+\]: https?://' }

$previous = git describe --tags --abbrev=0 "$Tag^" 2>$null
$range = if ($LASTEXITCODE -eq 0 -and $previous) { "$previous..$Tag" } else { $Tag }
$commits = git log --no-merges --format='%H %s' $range
if ($LASTEXITCODE -ne 0) { throw "git log $range failed" }

$groups = [ordered]@{ 'Features' = @(); 'Bug Fixes' = @(); 'Other' = @() }
foreach ($c in $commits) {
    $subject = $c.Substring(41)
    if ($subject -match '^release(\(.+\))?!?:') { continue }
    $group = if ($subject -match '^feat(\(.+\))?!?:') { 'Features' }
        elseif ($subject -match '^fix(\(.+\))?!?:') { 'Bug Fixes' }
        else { 'Other' }
    $groups[$group] += "* $c"
}

$notes = @(
    "## PHPev $Version"
    ''
    'PHPev is a fast, lightweight, open-source local PHP & Laravel environment for Windows:'
    'PHP, Composer and MariaDB in seconds, a version per project, and a'
    '`http://<project>.localhost` domain (with optional HTTPS) for every folder.'
)
$notes += $section
$notes += '## Changelog'
foreach ($name in $groups.Keys) {
    if ($groups[$name].Count) {
        $notes += "### $name"
        $notes += $groups[$name]
    }
}
$notes += ''
$notes += "**Full changelog**: https://github.com/duy-lee/PHPev/compare/$($range -replace '\.\.', '...')"
if ($range -eq $Tag) { $notes[-1] = "**Full changelog**: https://github.com/duy-lee/PHPev/commits/$Tag" }

New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
$notes | Out-File -Encoding utf8 $Out
Get-Content $Out

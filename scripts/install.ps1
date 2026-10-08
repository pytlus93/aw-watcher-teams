param(
    [Parameter(Mandatory=$true)][string]$ActivityWatchDirectory,
    [string]$Executable,
    [switch]$NoAutostart
)
$ErrorActionPreference = 'Stop'
if ($env:AW_PROFILE -and $env:AW_PROFILE -ne 'default') { throw 'Custom AW profiles are not supported yet' }
# Works both in the source checkout and alongside the executable in the ZIP.
if (-not $Executable) {
    $Executable = Join-Path $PSScriptRoot 'aw-watcher-teams.exe'
    if (-not (Test-Path -LiteralPath $Executable)) {
        $Executable = Join-Path (Split-Path -Parent $PSScriptRoot) 'target\release\aw-watcher-teams.exe'
    }
}
$installRoot = (Resolve-Path -LiteralPath $ActivityWatchDirectory).Path
if (-not (Test-Path -LiteralPath (Join-Path $installRoot 'aw-qt.exe') -PathType Leaf)) {
    throw 'ActivityWatchDirectory must contain aw-qt.exe'
}
if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) { throw 'Build the executable first' }
$destination = Join-Path $installRoot 'aw-watcher-teams'
New-Item -ItemType Directory -Path $destination -Force | Out-Null
$installedExe = Join-Path $destination 'aw-watcher-teams.exe'
Copy-Item -LiteralPath $Executable -Destination $installedExe -Force
$configPath = & $installedExe --init-config
if ($LASTEXITCODE -ne 0) { throw 'Config initialization failed' }
if (-not $NoAutostart) {
    $profile = $env:AW_PROFILE
    if ($profile -and $profile -ne 'default') {
        if ($profile -notmatch '^[A-Za-z0-9_-]+$') { throw 'Invalid AW_PROFILE' }
        $appName = "activitywatch-$profile"
    } else { $appName = 'activitywatch' }
    $qtDir = Join-Path (Join-Path (Join-Path $env:LOCALAPPDATA $appName) $appName) 'aw-qt'
    New-Item -ItemType Directory -Path $qtDir -Force | Out-Null
    $qtConfig = Join-Path $qtDir 'aw-qt.toml'
    $text = if (Test-Path -LiteralPath $qtConfig) { [IO.File]::ReadAllText($qtConfig) } else { '' }
    $section = [regex]::Match($text,'(?ms)^\[aw-qt\][^\r\n]*(?:\r?\n|\z)(?<body>.*?)(?=^\[|\z)')
    $defaults = 'autostart_modules = ["aw-server", "aw-watcher-afk", "aw-watcher-window", "aw-watcher-teams"]'
    if ($section.Success) {
        $body = $section.Groups['body'].Value
        $assignment = [regex]::Match($body,'(?ms)^\s*autostart_modules\s*=\s*\[(?<items>.*?)\]')
        if ($assignment.Success) {
            $items = $assignment.Groups['items'].Value
            $withoutComments = [regex]::Replace($items,'(?m)#[^\r\n]*','')
            if ($withoutComments -notmatch '(?:"aw-watcher-teams"|''aw-watcher-teams'')') {
                # Only handle ordinary arrays of quoted module names; do not rewrite unusual TOML.
                if ($withoutComments -notmatch '^\s*(?:(?:"[A-Za-z0-9_.-]+"|''[A-Za-z0-9_.-]+'')\s*,?\s*)*$') {
                    throw 'Unusual autostart_modules syntax: add aw-watcher-teams manually in aw-qt.toml'
                }
                $newItems = $withoutComments.Trim().TrimEnd(',').Trim()
                if ($newItems) { $newItems += ', ' }
                $replacement = 'autostart_modules = [' + $newItems + '"aw-watcher-teams"]'
                $body = $body.Substring(0,$assignment.Index) + $replacement + $body.Substring($assignment.Index+$assignment.Length)
            }
        } else { $body = "`r`n" + $defaults + "`r`n" + $body }
        $offset = $section.Groups['body'].Index
        $text = $text.Substring(0,$offset) + $body + $text.Substring($offset+$section.Groups['body'].Length)
    } else { $text += "`r`n[aw-qt]`r`n$defaults`r`n" }
    if (Test-Path -LiteralPath $qtConfig) { Copy-Item -LiteralPath $qtConfig -Destination "$qtConfig.before-teams" -Force }
    [IO.File]::WriteAllText($qtConfig,$text,[Text.UTF8Encoding]::new($false))
    Write-Host "AW autostart configured: $qtConfig"
}
Write-Host "Installed: $installedExe"
Write-Host "Local module settings: $configPath"
Write-Host 'Then restart ActivityWatch. Modules -> aw-watcher-teams controls the module.'

param([switch]$Test,[switch]$Package)
$ErrorActionPreference='Stop'
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
 if($Test){
  & cargo test --offline --locked
  if($LASTEXITCODE -ne 0){throw 'Tests failed'}
  & cargo clippy --offline --all-targets -- -D warnings
  if($LASTEXITCODE -ne 0){throw 'Clippy failed'}
 }
 & cargo build --offline --release --locked
 if($LASTEXITCODE -ne 0){throw 'Build failed'}
 $taskManifest='aw-watcher-teams.exe.actions.json'
 $taskActions=[IO.File]::ReadAllText((Join-Path (Get-Location) $taskManifest)) | ConvertFrom-Json
 if($taskActions.schema_version -ne 1 -or $taskActions.module -ne 'aw-watcher-teams'){throw 'Invalid module actions manifest'}
 Copy-Item -LiteralPath $taskManifest -Destination ('target\release\'+$taskManifest) -Force
 if($Package){
  $metadata=& cargo metadata --offline --no-deps --format-version 1 | ConvertFrom-Json
  if($LASTEXITCODE -ne 0){throw 'Package metadata unavailable'}
  $version=($metadata.packages | Where-Object name -eq 'aw-watcher-teams').version
  $stage=Join-Path 'dist' "aw-watcher-teams-$version"
  New-Item -ItemType Directory -Path $stage -Force|Out-Null
  Copy-Item 'target\release\aw-watcher-teams.exe' $stage -Force
  Copy-Item 'target\release\aw-watcher-teams.exe.actions.json' $stage -Force
  Copy-Item 'scripts\install.ps1' $stage -Force
  Copy-Item 'README.md' $stage -Force
  New-Item -ItemType Directory -Path (Join-Path $stage 'docs') -Force | Out-Null
  Copy-Item 'docs\*' (Join-Path $stage 'docs') -Recurse -Force
  Compress-Archive -Path (Join-Path $stage '*') -DestinationPath "$stage.zip" -Force
  Write-Output "$stage.zip"
 }
}finally{Pop-Location}

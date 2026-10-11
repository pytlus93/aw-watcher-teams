param([string]$ConfigPath, [switch]$ValidateOnly)
$ErrorActionPreference='Stop'
if (-not $ConfigPath) { $ConfigPath=Join-Path $env:LOCALAPPDATA 'activitywatch\aw-watcher-teams\config.json' }
$config = [IO.File]::ReadAllText($ConfigPath) | ConvertFrom-Json
if (-not $config.PSObject.Properties['title_format']) { $config | Add-Member title_format ([pscustomobject]@{include_organization=$true;include_account=$false;organization_map=[pscustomobject]@{};separator=' | '}) }
if (-not $config.PSObject.Properties['collect_meeting_title']) { $config | Add-Member collect_meeting_title $false }
$defaults=[ordered]@{schema_version=$true;app=$true;meeting_title=$true;organization=$true;account=$false;call_kind=$true;state=$true;assessment=$true;evidence=$true;title_source=$true;session_id=$true;identity_source=$true}
if (-not $config.PSObject.Properties['payload']) { $config | Add-Member payload ([pscustomobject]@{mode='strict';fields=[pscustomobject]$defaults}) }
if (-not $config.payload.PSObject.Properties['mode']) { $config.payload | Add-Member mode 'strict' }
if (-not $config.payload.PSObject.Properties['fields']) { $config.payload | Add-Member fields ([pscustomobject]$defaults) }
foreach ($name in $defaults.Keys) { if (-not $config.payload.fields.PSObject.Properties[$name]) {$config.payload.fields | Add-Member $name $defaults[$name]} }
if ($ValidateOnly) { $config | ConvertTo-Json -Depth 10 -Compress; return }
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$form=New-Object Windows.Forms.Form
$form.Text='MS Teams module settings'
$form.ClientSize=New-Object Drawing.Size(550,650)
$form.StartPosition='CenterScreen'
$layout=New-Object Windows.Forms.FlowLayoutPanel
$layout.Dock='Fill';$layout.FlowDirection='TopDown';$layout.WrapContents=$false;$layout.AutoScroll=$true
$form.Controls.Add($layout)
$note=New-Object Windows.Forms.Label
$note.Text="Strict sends only title (this custom bucket has no official data fields). Extended adds checked fields. Privacy choices also apply to titles. Restart the module through AW after saving."
$note.Size=New-Object Drawing.Size(510, 60)
$note.Height=60
$layout.Controls.Add($note)
$mode=New-Object Windows.Forms.ComboBox
$mode.DropDownStyle='DropDownList';[void]$mode.Items.AddRange(@('strict','extended'))
$mode.SelectedItem=$config.payload.mode
$layout.Controls.Add($mode)
$capture=New-Object Windows.Forms.CheckBox
$capture.Text='Read qualified call captions';$capture.Width=510;$capture.Checked=[bool]$config.collect_meeting_title
$layout.Controls.Add($capture)
$checks=@{}
$labels=@{schema_version='Schema version';app='Application label';meeting_title='Topic (also permits it in title)';organization='Organization (also permits it in title)';account='Account email (private; also permits it in title)';call_kind='Call or meeting classification';state='Detection state';assessment='Inference assessment';evidence='Evidence category';title_source='Title provenance';session_id='Local interval identity';identity_source='Identity provenance'}
foreach ($name in $defaults.Keys) {
 $check=New-Object Windows.Forms.CheckBox
 $check.Text=$labels[$name];$check.Width=510;$check.Checked=[bool]$config.payload.fields.$name
 $checks[$name]=$check;$layout.Controls.Add($check)
}
$organization=New-Object Windows.Forms.CheckBox
$organization.Text='Include organization in formatted title';$organization.Width=510;$organization.Checked=[bool]$config.title_format.include_organization
$layout.Controls.Add($organization)
$account=New-Object Windows.Forms.CheckBox
$account.Text='Include account in formatted title';$account.Width=510;$account.Checked=[bool]$config.title_format.include_account
$layout.Controls.Add($account)
$save=New-Object Windows.Forms.Button;$save.Text='Save';$layout.Controls.Add($save)
$save.Add_Click({
 try {
  $config.collect_meeting_title=$capture.Checked;$config.payload.mode=[string]$mode.SelectedItem
  foreach($name in $defaults.Keys){$config.payload.fields.$name=$checks[$name].Checked}
  $config.title_format.include_organization=$organization.Checked;$config.title_format.include_account=$account.Checked
  $temp=$ConfigPath+'.settings-'+[Guid]::NewGuid().ToString('N')
  [IO.File]::WriteAllText($temp,($config|ConvertTo-Json -Depth 10),[Text.UTF8Encoding]::new($false))
  & $env:AW_TEAMS_SETTINGS_EXE --check-config $temp
  if($LASTEXITCODE -ne 0){throw 'Settings validation failed'}
  $backup=$ConfigPath+'.before-settings-'+(Get-Date -Format 'yyyyMMdd-HHmmss')
  [IO.File]::Replace($temp,$ConfigPath,$backup)
  $form.Close()
 } catch { [void][Windows.Forms.MessageBox]::Show('Settings could not be saved. Check field choices and configuration.','MS Teams settings') }
})
[void]$form.ShowDialog()

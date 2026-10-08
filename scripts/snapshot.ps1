param([switch]$CollectMeetingTitle)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
# Only button names and qualified call-window captions. No chat/body/roster traversal.
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class TeamsWindows {
 public delegate bool Callback(IntPtr h, IntPtr p);
 [DllImport("user32.dll")] public static extern bool EnumWindows(Callback cb, IntPtr p);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder text, int count);
}
'@
try {
 $pids = @{}
 Get-Process -Name ms-teams -ErrorAction SilentlyContinue | ForEach-Object {
  $pids[[uint32]$_.Id] = "$($_.Id):$($_.StartTime.ToUniversalTime().Ticks)"
 }
 $handles = [Collections.Generic.List[IntPtr]]::new()
 $cb = [TeamsWindows+Callback]{
  param($h,$p)
  [uint32]$owner=0
  [void][TeamsWindows]::GetWindowThreadProcessId($h,[ref]$owner)
  if($pids.ContainsKey($owner)) { $handles.Add($h) }
  return $true
 }
 [void][TeamsWindows]::EnumWindows($cb,[IntPtr]::Zero)
 $windows = @()
 $allOk = $true
 foreach($h in $handles) {
  try {
   [uint32]$owner=0
   [void][TeamsWindows]::GetWindowThreadProcessId($h,[ref]$owner)
   $element=[Windows.Automation.AutomationElement]::FromHandle($h)
   $condition=[Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::ControlTypeProperty,
    [Windows.Automation.ControlType]::Button)
   $buttons=$element.FindAll([Windows.Automation.TreeScope]::Descendants,$condition)
   $names=@()
   foreach($b in $buttons) { $names += $b.Current.Name }
   $leave=(@($names | Where-Object { $_ -match '^(Leave|Hang up|Opustit|Zavěsit)(\s|$)' }).Count -gt 0)
   $microphone=(@($names | Where-Object { $_ -match '^(Mute|Unmute|Ztlumit|Zapnout mikrofon)(\s|$)' }).Count -gt 0)
   $camera=(@($names | Where-Object { $_ -match '^(Turn camera on|Turn camera off|Zapnout kameru|Vypnout kameru)(\s|$)' }).Count -gt 0)
   $join=(@($names | Where-Object { $_ -match '^(Join now|Připojit se)(\s|$)' }).Count -gt 0)
   $sharing=(@($names | Where-Object { $_ -match '^(Stop sharing|Ukončit sdílení)(\s|$)' }).Count -gt 0)
   $meetingInfo=(@($names | Where-Object { $_ -match '^(Meeting info|Meeting information|Meeting details|Informace o schůzce|Podrobnosti schůzky)(\s|$)' }).Count -gt 0)
   $title=$null
   if($CollectMeetingTitle -and $leave -and ($microphone -or $camera -or $sharing) -and -not $join) {
    $caption=[Text.StringBuilder]::new(512)
    [void][TeamsWindows]::GetWindowText($h,$caption,$caption.Capacity)
    $title=$caption.ToString()
    if([string]::IsNullOrWhiteSpace($title) -or $title -in @('Microsoft Teams','Teams','MS Teams')) {
     $title=$element.Current.Name
    }
   }
   $windows += @{
    key="$($pids[$owner]):$($h.ToInt64())";leave=$leave;microphone=$microphone;camera=$camera
    join=$join;sharing=$sharing;meeting_info=$meetingInfo;title=$title
   }
  } catch { $allOk=$false }
 }
 @{collector_ok=$allOk;windows=@($windows)} | ConvertTo-Json -Depth 5 -Compress
} catch {
 # Never emit exception text or unfiltered UI data.
 '{"collector_ok":false,"windows":[]}'
}

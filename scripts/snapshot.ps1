$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
# No text/document/edit patterns, messages, window titles or participant names read.
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class TeamsWindows {
 public delegate bool Callback(IntPtr h, IntPtr p);
 [DllImport("user32.dll")] public static extern bool EnumWindows(Callback cb, IntPtr p);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
}
'@
try {
 $pids = @{}
 Get-Process -Name ms-teams -ErrorAction SilentlyContinue | ForEach-Object { $pids[[uint32]$_.Id] = $true }
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
 foreach($h in $handles) {
  $element=[Windows.Automation.AutomationElement]::FromHandle($h)
  $condition=[Windows.Automation.PropertyCondition]::new(
   [Windows.Automation.AutomationElement]::ControlTypeProperty,
   [Windows.Automation.ControlType]::Button)
  $buttons=$element.FindAll([Windows.Automation.TreeScope]::Descendants,$condition)
  $names=@()
  foreach($b in $buttons) { $names += $b.Current.Name }
  # Exact allowlist only. Localization profiles require validation.
  $windows += @{
   leave=(@($names | Where-Object { $_ -in @('Leave','Hang up','Opustit','Zavěsit') }).Count -gt 0)
   microphone=(@($names | Where-Object { $_ -match '^(Mute|Unmute|Ztlumit|Zapnout mikrofon)( |$)' }).Count -gt 0)
   camera=(@($names | Where-Object { $_ -match '^(Turn camera on|Turn camera off|Zapnout kameru|Vypnout kameru)( |$)' }).Count -gt 0)
   join=(@($names | Where-Object { $_ -in @('Join now','Připojit se') }).Count -gt 0)
   sharing=(@($names | Where-Object { $_ -in @('Stop sharing','Ukončit sdílení') }).Count -gt 0)
  }
 }
 @{collector_ok=$true;windows=@($windows)} | ConvertTo-Json -Depth 5 -Compress
} catch {
 # Do not emit exception text containing private UI data.
 '{"collector_ok":false,"windows":[]}'
}

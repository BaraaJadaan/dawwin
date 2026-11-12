# Find the native dialog of process $procId titled $title, say its text, and press $button.
param([int]$procId, [string]$title, [string]$button, [int]$buttonId)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]
$root = $A::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition($A::ProcessIdProperty, $procId)
$dlg = $null
for ($i = 0; $i -lt 60 -and -not $dlg; $i++) {
  foreach ($w in $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)) {
    if ($w.Current.Name -eq $title -and $w.Current.ClassName -eq '#32770') { $dlg = $w }
  }
  if (-not $dlg) { Start-Sleep -Milliseconds 250 }
}
if (-not $dlg) { Write-Output "NO DIALOG"; exit 1 }
$all = $dlg.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$names = @($all | ForEach-Object { $_.Current.Name } | Where-Object { $_ })
if (-not ($names -contains $button)) { Write-Output ("NO BUTTON; " + ($names -join ' | ')); exit 1 }
Add-Type -Namespace W -Name U -MemberDefinition '[DllImport("user32.dll")] public static extern System.IntPtr SendMessageW(System.IntPtr h, uint m, System.IntPtr w, System.IntPtr l);'
# TDM_CLICK_BUTTON: the task dialog presses its own button, without the mouse or the focus
[void][W.U]::SendMessageW([System.IntPtr]$dlg.Current.NativeWindowHandle, 0x466, [System.IntPtr]$buttonId, [System.IntPtr]::Zero)
[Console]::OutputEncoding = [Text.Encoding]::UTF8
Write-Output ($names -join ' | ')

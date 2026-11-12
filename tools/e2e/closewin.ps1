# Close the main window of process $procId the way its X button does (WM_CLOSE).
param([int]$procId)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]
$cond = New-Object System.Windows.Automation.PropertyCondition($A::ProcessIdProperty, $procId)
Add-Type -Namespace W -Name U -MemberDefinition '[DllImport("user32.dll")] public static extern bool PostMessageW(System.IntPtr h, uint m, System.IntPtr w, System.IntPtr l);'
foreach ($w in $A::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)) {
  if ($w.Current.ClassName -eq 'Tauri Window') { [void][W.U]::PostMessageW([System.IntPtr]$w.Current.NativeWindowHandle, 0x10, [System.IntPtr]::Zero, [System.IntPtr]::Zero); Write-Output ("closed " + $w.Current.ClassName) }
}

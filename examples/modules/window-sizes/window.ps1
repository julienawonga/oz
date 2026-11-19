# Réduit, agrandit ou restaure une fenêtre.
# Oz fournit $OzTool (l'outil appelé) et $OzArgs (ses arguments).
# La fenêtre est cherchée par un morceau de son titre ou le nom de son appli :
# le nom que Julien emploie (« le terminal ») n'est presque jamais le titre exact.

Add-Type -Namespace OzWin -Name User32 -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool ShowWindow(System.IntPtr hWnd, int nCmdShow);
'@

$target = "$($OzArgs.target)".Trim()
if (-not $target) { Write-Output "Échec : aucune fenêtre indiquée."; exit 1 }

$window = Get-Process |
  Where-Object { $_.MainWindowHandle -ne 0 -and ($_.MainWindowTitle -like "*$target*" -or $_.ProcessName -like "*$target*") } |
  Select-Object -First 1

if (-not $window) { Write-Output "Introuvable : aucune fenêtre ne correspond à « $target »."; exit 1 }

# 6 = réduire, 3 = agrandir, 9 = restaurer
$cmd = switch ($OzTool) { 'minimize_window' { 6 } 'maximize_window' { 3 } default { 9 } }
$verb = switch ($cmd) { 6 { 'réduite' } 3 { 'agrandie' } default { 'restaurée' } }

[void][OzWin.User32]::ShowWindow($window.MainWindowHandle, $cmd)
Write-Output "Fenêtre $verb : « $($window.MainWindowTitle) » ($($window.ProcessName))."

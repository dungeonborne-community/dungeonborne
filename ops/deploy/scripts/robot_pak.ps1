$ErrorActionPreference = 'Stop'
$g = 'C:\DungeonbornServer\Dungeonborne\Dungeonborne'
$k = 'C:\DungeonbornServer\Kit'
$u = 'C:\DungeonbornServer\upload_20261114'
$b = 'C:\DungeonbornServer\Kit\backup_before_20261114'
$pak = "$g\Content\Paks\zzzzzz_Balance_P.pak"
if ((Get-FileHash (Join-Path $u 'zzzzzz_Balance_P.pak')).Hash.Substring(0,16) -ne 'D8D44788195DCD01') { throw 'upload sha mismatch' }
schtasks /End /TN DbAgent | Out-Null
& (Join-Path $k 'start_headless.ps1') -Quit -All | Out-Null
$t = 0; while ((Get-Process Dungeonborne -ErrorAction SilentlyContinue) -and $t -lt 120) { Start-Sleep 3; $t += 3 }
"game processes left: " + (Get-Process Dungeonborne -ErrorAction SilentlyContinue | Measure-Object).Count + " after $t s"
if (Get-Process Dungeonborne -ErrorAction SilentlyContinue) { throw 'game still running - stop' }
New-Item -ItemType Directory -Force -Path $b | Out-Null
Copy-Item $pak (Join-Path $b 'zzzzzz_Balance_P.pak') -Force
"backup: " + (Get-FileHash (Join-Path $b 'zzzzzz_Balance_P.pak')).Hash.Substring(0,16)
Copy-Item (Join-Path $u 'zzzzzz_Balance_P.pak') $pak -Force
"new: " + (Get-FileHash $pak).Hash.Substring(0,16)
schtasks /Run /TN DbAgent | Out-Null
Start-Sleep 20
Get-Process ds-agent, Dungeonborne -ErrorAction SilentlyContinue | Select-Object Id, ProcessName, StartTime | Format-Table -AutoSize | Out-String

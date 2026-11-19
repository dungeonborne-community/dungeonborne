# 10.11: DbAgent task — at administrator logon + every 5 minutes + restart on failure.
# An extra agent instance exits right away (lock 127.0.0.1:9095), so a repeated start is safe.
# Run: PowerShell as administrator on Robot, in the RDP session of the user the servers run under.
$ErrorActionPreference = 'Stop'
$exe = 'C:\DungeonbornServer\Kit\ds-agent.exe'
# over SSH USERDOMAIN = WORKGROUP (10.11: "No mapping between account names") — the name comes from the process token
$user = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
$action = New-ScheduledTaskAction -Execute $exe -WorkingDirectory 'C:\DungeonbornServer\Kit'
$logon = New-ScheduledTaskTrigger -AtLogOn -User $user
$every = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(1) -RepetitionInterval (New-TimeSpan -Minutes 5)
# Interactive: the game needs a user session (like the former /SC ONLOGON task)
$principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Highest
$settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit ([TimeSpan]::Zero) `
    -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -StartWhenAvailable `
    -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName 'DbAgent' -Action $action -Trigger @($logon, $every) -Principal $principal `
    -Settings $settings -Force | Out-Null
Get-ScheduledTask -TaskName 'DbAgent' | Select-Object TaskName, State
(Get-ScheduledTask -TaskName 'DbAgent').Triggers | Select-Object @{n = 'Trigger'; e = { $_.CimClass.CimClassName } }, @{n = 'Every'; e = { $_.Repetition.Interval } }

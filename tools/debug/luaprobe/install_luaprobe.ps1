# Debug pak of the client Lua probe (docs/CLIENT_PROBE.md): installs/removes zzzzzzzz_LuaProbe_P.pak in local installs.
# Only with the game closed. Does not go into the launcher (make_client_bundle.py skips *LuaProbe*), do not install on Robot.
#   powershell -ExecutionPolicy Bypass -File debug\luaprobe\install_luaprobe.ps1            build and install
#   ... -Remove                                                                             remove from all installs
# The probe itself writes nothing until enabled: ue4ss/Mods/ClientProbe/lua_config.txt (enabled = 1) or the -dbprobe flag.
param([switch]$Remove)
$ErrorActionPreference = 'Stop'
if (Get-Process Dungeonborne -ErrorAction SilentlyContinue) { Write-Host 'The game is running — close it and try again.'; exit 1 }
$here = $PSScriptRoot
. "$here\..\..\stand\paths.ps1"   # $Games (local.toml [games]), Get-LocalValue, Resolve-WsPath
$name = 'zzzzzzzz_LuaProbe_P'
$repak = Resolve-WsPath $(if (Get-LocalValue 'tools' 'repak') { Get-LocalValue 'tools' 'repak' } else { 'home:tools/repak/repak.exe' })
if (-not $Remove) {
    & $repak pack --version V8B --mount-point ../../../ "$here\$name" "$here\$name.pak"
    if ($LASTEXITCODE -ne 0) { exit 1 }
}
foreach ($g in $Games.Values) {   # all installs from local.toml [games]
    $paks = "$g\Dungeonborne\Content\Paks"
    if (-not (Test-Path $paks)) { continue }
    if ($Remove) {
        Remove-Item "$paks\$name.pak" -ErrorAction SilentlyContinue
        Write-Host "removed: $paks"
    } else {
        Copy-Item "$here\$name.pak" "$paks\$name.pak" -Force
        Write-Host "installed: $paks"
    }
}

# Records which source the dll was built from: dbnative.build.sha256 (tracked in git).
# cargo xtask mods check compares it with the current dbnative.c and dbnative.dll instead of file times
# (after git clone the source gets the clone time and would look newer than the dll).
$here = $PSScriptRoot
$c = (Get-FileHash -Algorithm SHA256 (Join-Path $here 'dbnative.c')).Hash.ToLower()
$d = (Get-FileHash -Algorithm SHA256 (Join-Path $here '..\..\Mods\DungeonborneRuClient\native\dbnative.dll')).Hash.ToLower()
[IO.File]::WriteAllText((Join-Path $here 'dbnative.build.sha256'), "c $c`nd $d`n")
"stamp: c $($c.Substring(0,12)) d $($d.Substring(0,12))"

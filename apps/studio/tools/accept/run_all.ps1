# Full acceptance (section 9 of docs/BALANCE_APP_SPEC.md) on the built exe. Writes only under the target folders (CARGO_TARGET_DIR, or apps\studio\target and core\backend\target).
#   powershell -NoProfile -File tools\accept\run_all.ps1
$ErrorActionPreference = 'Continue'
$env:PYTHONUTF8 = '1'
$root = (Resolve-Path "$PSScriptRoot\..\..").Path    # apps\studio
$ws = (Resolve-Path "$root\..\..").Path              # workspace root (docs/WORKSPACE_PLAN.md)
$acc = "$root\tools\accept"
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$root\target" }
$xtarget = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$ws\core\backend\target" }
$exe = "$target\release\balance-studio.exe"
$work = "$target\accept"
# The newest kits; the season one — of the active season (Dead Money was removed 07.11)
function Newest($pattern) { (Get-ChildItem $pattern | Sort-Object LastWriteTime | Select-Object -Last 1).FullName -replace '\.zip$', '' }
$base = Newest "$ws\content\balance\kit\out\dungeonborne-balance-*.zip"
$seasonId = ((Get-Content "$ws\content\seasons\active.txt" -TotalCount 1) -split '\s+')[0]
$season = Newest "$ws\content\seasons\$seasonId\kit\$seasonId-balance-*.zip"
$port = 9334
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$port"

function Start-App($name) {
  Get-Process balance-studio -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Seconds 1
  $d = "$work\$name"
  New-Item -ItemType Directory -Force "$d\shots" | Out-Null
  $env:BALANCE_STUDIO_DATA = $d
  # The checks expect the Russian interface and Russian kit texts; English is the default since 18.11
  Set-Content -LiteralPath "$d\settings.json" -Value '{"lang":"ru"}' -Encoding ascii
  $script:proc = Start-Process $exe -PassThru
  Start-Sleep -Seconds 4
  return $d
}
function Show($lines) { $lines | ForEach-Object { "$_" } | Where-Object { $_ -match '^(BAD|FAILS?|FAILED|problems|origins|error bar|Traceback)' } | ForEach-Object { $_.Substring(0, [Math]::Min(260, $_.Length)) } }

if (Test-Path $work) { Remove-Item $work -Recurse -Force }
New-Item -ItemType Directory -Force $work | Out-Null

'=== keyboard and mouse (M2)'
$d = Start-App 'm2'
Show (& node "$acc\cdp_m2_ui.mjs" $port "$base.zip" "$d\shots" 2>&1)

'=== item card, curves, bulk (M3)'
$d = Start-App 'm3'
Show (& node "$acc\cdp_m3_ui.mjs" $port "$base.zip" "$d\shots" 2>&1)

'=== moving changes, patches, small window, English (M4)'
$d = Start-App 'm4'
& python "$acc\make_m4_kit.py" "$base.zip" "$d\new_kit.zip"
Show (& node "$acc\cdp_m4.mjs" $port "$base.zip" "$d\new_kit.zip" "$acc\bad_patch.json" "$d\patch_moved.json" "$d\shots" 2>&1)
"check.py on the moved patch: $((& python "$base\tools\check.py" "$d\patch_moved.json" --kit ((Get-ChildItem "$d\kits" -Directory | Sort-Object Name | Select-Object -Last 1).FullName) 2>&1 | Select-Object -Last 1))"

'=== system file dialogs'
$d = Start-App 'dialogs'
Show (& powershell -NoProfile -File "$acc\m4_dialogs.ps1" $port $proc.Id "$base.zip" $base $d "$acc\cdp_eval.mjs" 2>&1)

'=== loot screen, and the drop against loot show on every map: base and season'
foreach ($k in @(@($base, 'none'), @($season, $seasonId))) {
  $d = Start-App ("loot-" + (Split-Path $k[0] -Leaf))
  "$(Split-Path $k[0] -Leaf):"
  Show (& node "$acc\cdp_s2_loot.mjs" $port "$($k[0]).zip" "$d\shots" 2>&1)
  Show (& python "$acc\s2_show_compare.py" "$d\shots\drop.json" $k[1] 2>&1)
}

'=== loot patch on a copy of the project: base and season'
Show (& python "$acc\s2_loot_accept.py" $base $season 2>&1)

'=== compatibility with the book and the server: base and season'
foreach ($kit in $base, $season) {
  $d = Start-App ("kit-" + (Split-Path $kit -Leaf))
  & python "$acc\m2_gen.py" $kit "$d\edits.json" | Out-Null
  & node "$acc\cdp_m2_export.mjs" $port "$kit.zip" "$d\edits.json" "$d\patch.json" 2>&1 | Out-Null
  "$(Split-Path $kit -Leaf):"
  Show (& python "$acc\m2_accept.py" $kit "$d\edits.json" "$d\patch.json" $d 2>&1)
}
Remove-Item "$xtarget\m2-accept" -Recurse -Force -ErrorAction SilentlyContinue
Get-Process balance-studio -ErrorAction SilentlyContinue | ForEach-Object { $_.CloseMainWindow() | Out-Null }
'=== done'

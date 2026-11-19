# The real system file dialogs of the exe: buttons pressed in the page, dialogs answered through window messages.
# powershell -File m4_dialogs.ps1 <port> <app pid> <kit.zip> <kit folder> <working folder> <cdp_eval.mjs>
param($Port, $AppPid, $KitZip, $KitDir, $Work, $Eval)
$kitVersion = '"' + (Get-Content "$KitDir\manifest.json" -Raw -Encoding UTF8 | ConvertFrom-Json).version + '"'
$failed = 0

# PowerShell 5.1 drops double quotes from arguments of native programs unless they are escaped
function Page($js) { (& node $Eval $Port ($js -replace '"', '\"')) -join "`n" }

# The dialog's controls expose no UI Automation patterns here, so they are driven by window messages: no focus is needed
Add-Type -TypeDefinition @"
using System; using System.Collections.Generic; using System.Runtime.InteropServices; using System.Text;
public static class Dlg {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr p, EnumProc f, IntPtr l);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern IntPtr GetDlgItem(IntPtr h, int id);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, string l);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  static string Class(IntPtr h) { var s = new StringBuilder(64); GetClassName(h, s, 64); return s.ToString(); }
  public static string Title(IntPtr h) { var s = new StringBuilder(128); GetWindowText(h, s, 128); return s.ToString(); }
  public static IntPtr Find(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); if (p == pid && IsWindowVisible(h) && Class(h) == "#32770") { found = h; return false; } return true; }, IntPtr.Zero);
    return found;
  }
  // The file name field is the lowest visible Edit window in the dialog
  public static bool SetName(IntPtr d, string text) {
    IntPtr best = IntPtr.Zero; int bestY = -1;
    EnumChildWindows(d, (h, l) => { if (Class(h) == "Edit" && IsWindowVisible(h)) { RECT r; GetWindowRect(h, out r); if (r.T > bestY) { bestY = r.T; best = h; } } return true; }, IntPtr.Zero);
    if (best == IntPtr.Zero) return false;
    SendMessage(best, 0x000C, IntPtr.Zero, text);
    return true;
  }
  public static void Press(IntPtr d, int id) { PostMessage(GetDlgItem(d, id), 0x00F5, IntPtr.Zero, IntPtr.Zero); }
}
"@

function Find-Dialog {
  for ($i = 0; $i -lt 60; $i++) {
    $h = [Dlg]::Find([uint32]$AppPid)
    if ($h -ne [IntPtr]::Zero) { Start-Sleep -Milliseconds 500; return $h }
    Start-Sleep -Milliseconds 250
  }
  throw 'no dialog appeared'
}

function Wait-Closed {
  for ($i = 0; $i -lt 40; $i++) {
    if ([Dlg]::Find([uint32]$AppPid) -eq [IntPtr]::Zero) { return }
    Start-Sleep -Milliseconds 250
  }
  throw 'the dialog stayed open'
}

function Answer($path) {
  $d = Find-Dialog
  "   dialog: $([Dlg]::Title($d))"
  if ($path) {
    if (-not [Dlg]::SetName($d, $path)) { throw 'no file name field' }
    Start-Sleep -Milliseconds 200
    [Dlg]::Press($d, 1)
  } else {
    [Dlg]::Press($d, 2)
  }
  Wait-Closed
}

function Check($name, $got, $want) {
  if ($got -eq $want) { "ok  $name" } else { $script:failed++; "BAD $name`: got $got, want $want" }
}

# Whatever dialog was left from an interrupted run
$left = [Dlg]::Find([uint32]$AppPid)
if ($left -ne [IntPtr]::Zero) { [Dlg]::Press($left, 2); Wait-Closed }

# Cancel changes nothing
Page 'document.getElementById("open-zip").click()' | Out-Null
Answer $null
Start-Sleep -Milliseconds 500
Check 'cancel: no kit opened' (Page 'S.kit ? "open" : "none"') '"none"'

# The zip button
Page 'document.getElementById("open-zip").click()' | Out-Null
Answer $KitZip
Start-Sleep -Seconds 2
Check 'zip through the dialog' (Page 'S.kit?.version') $kitVersion

# Save: fills the suggested name, writes the patch where asked
Page '(async () => { await invoke("set_values", { edits: [{ table: "gear_template", row: 0, path: "durability", new: "120" }] }); showView("changes"); await new Promise((r) => setTimeout(r, 300)); const a = document.getElementById("ch-author"); a.value = "m4"; a.dispatchEvent(new Event("input")); await new Promise((r) => setTimeout(r, 500)); document.getElementById("ch-save").click(); return 1; })()' | Out-Null
$patch = Join-Path $Work 'saved-by-dialog.json'
if (Test-Path $patch) { Remove-Item $patch }
Answer $patch
Start-Sleep -Seconds 1
Check 'patch saved through the dialog' (Test-Path $patch) $true
"   notice: $(Page 'document.getElementById("ch-notice-text").textContent')"
$saved = Get-Content $patch -Raw -Encoding UTF8 | ConvertFrom-Json
Check 'version in the patch' ($saved.app -like 'balance-studio/*') $true
Check 'author in the patch' $saved.author 'm4'

# Open: the same patch through the dialog, after undoing the change
Page 'invoke("undo").then(updateStatus)' | Out-Null
Page 'document.getElementById("ch-open").click()' | Out-Null
Answer $patch
Start-Sleep -Seconds 1
"   notice: $(Page 'document.getElementById("ch-notice-text").textContent')"
Check 'patch opened through the dialog' (Page 'invoke("list_changes").then((d) => d.count)') '1'

# The folder button
Page '(async () => { await invoke("close_kit"); S.kit = null; G.table = null; showPage(); document.getElementById("open-folder").click(); return 1; })()' | Out-Null
Answer $KitDir
Start-Sleep -Seconds 2
Check 'folder through the dialog' (Page 'S.kit?.version') $kitVersion

"FAILED $failed"

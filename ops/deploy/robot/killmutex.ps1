# Removes the single-instance mutexes from all running Dungeonborne.exe so that a second one can start.
# Needs administrator rights (the game under ACE runs elevated). Started by the Task Scheduler task
# DbKillMutex from C:\ProgramData\DbTools (an administrators-only folder), see dbtest\README_admin.md.
#   -List  - only show the mutexes found, close nothing.
# The result is written to C:\ProgramData\DbTools\killmutex.log (read by dbtest\run.ps1).
param([switch]$List)

$LogPath = Join-Path $PSScriptRoot "killmutex.log"
$Match = '\\BaseNamedObjects\\(Dungeonborne|UnrealEngine4|CDdf212806D6EmB31yE0c)$'

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class HK {
    [DllImport("ntdll.dll")] static extern int NtQuerySystemInformation(int c, IntPtr b, int l, out int r);
    [DllImport("ntdll.dll")] static extern int NtQueryObject(IntPtr h, int c, IntPtr b, int l, out int r);
    [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr OpenProcess(int a, bool i, int p);
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool DuplicateHandle(IntPtr sp, IntPtr sh, IntPtr tp, out IntPtr th, int a, bool i, int o);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);

    static string Str(IntPtr h, int cls) {
        int len = 0x1000; IntPtr buf = Marshal.AllocHGlobal(len);
        try {
            int r; if (NtQueryObject(h, cls, buf, len, out r) != 0) return null;
            int l = Marshal.ReadInt16(buf); if (l == 0) return "";
            return Marshal.PtrToStringUni(Marshal.ReadIntPtr(buf, IntPtr.Size), l / 2);
        } finally { Marshal.FreeHGlobal(buf); }
    }

    public static List<string> Run(int pid, bool close, string match) {
        var res = new List<string>();
        int len = 0x100000; IntPtr buf; int r;
        while (true) {
            buf = Marshal.AllocHGlobal(len);
            int st = NtQuerySystemInformation(64, buf, len, out r); // SystemExtendedHandleInformation
            if (st == unchecked((int)0xC0000004)) { Marshal.FreeHGlobal(buf); len = Math.Max(len * 2, r + 0x10000); continue; }
            if (st != 0) throw new Exception("NtQSI 0x" + st.ToString("X"));
            break;
        }
        IntPtr hp = OpenProcess(0x0040 | 0x0400, false, pid); // DUP_HANDLE | QUERY_INFO
        if (hp == IntPtr.Zero) { Marshal.FreeHGlobal(buf); throw new Exception("OpenProcess failed " + Marshal.GetLastWin32Error()); }
        long count = Marshal.ReadIntPtr(buf).ToInt64();
        int entry = IntPtr.Size == 8 ? 40 : 28, off = IntPtr.Size * 2;
        for (long i = 0; i < count; i++) {
            IntPtr e = buf + off + (int)(i * entry);
            long epid = Marshal.ReadIntPtr(e, IntPtr.Size).ToInt64();
            if (epid != pid) continue;
            IntPtr hv = Marshal.ReadIntPtr(e, IntPtr.Size * 2);
            IntPtr dup;
            if (!DuplicateHandle(hp, hv, GetCurrentProcess(), out dup, 0, false, 2)) continue;
            string tn = Str(dup, 2); // ObjectTypeInformation: UNICODE_STRING at the start
            string name = tn == "Mutant" ? Str(dup, 1) : null;
            CloseHandle(dup);
            if (string.IsNullOrEmpty(name) || !System.Text.RegularExpressions.Regex.IsMatch(name, match)) continue;
            string line = "pid " + pid + " 0x" + hv.ToInt64().ToString("X") + " " + name;
            if (close) {
                IntPtr d2;
                bool ok = DuplicateHandle(hp, hv, IntPtr.Zero, out d2, 0, false, 1); // DUPLICATE_CLOSE_SOURCE
                line += ok ? "  -> closed" : "  -> FAILED";
            }
            res.Add(line);
        }
        CloseHandle(hp); Marshal.FreeHGlobal(buf);
        return res;
    }
}
"@

$out = @("=== " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss") + $(if ($List) { " (list)" } else { " (close)" }))
$procs = @(Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue)
if ($procs.Count -eq 0) { $out += "no Dungeonborne.exe running" }
foreach ($p in $procs) {
    try {
        $lines = [HK]::Run($p.Id, -not $List, $Match)
        if ($lines.Count -eq 0) { $out += "pid $($p.Id): no matching mutexes" } else { $out += $lines }
    } catch { $out += "pid $($p.Id): ERROR $($_.Exception.Message)" }
}
$out | Set-Content -Path $LogPath -Encoding UTF8
$out

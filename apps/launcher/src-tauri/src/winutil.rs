//! Thin wrappers over the few Win32 calls the launcher needs: ShellExecuteEx (UAC), process list,
//! port owner lookup, opening a URL in the system browser.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
};
use windows_sys::Win32::Networking::WinSock::AF_INET;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject, INFINITE,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Shell::{
    ShellExecuteExW, ShellExecuteW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};

use crate::error::{bail, Error, Result};

pub fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

/// Process handle that can cross threads. Closed on drop.
pub struct Proc(usize);

impl Proc {
    /// Blocks until the process exits; returns its exit code.
    pub fn wait(&self) -> u32 {
        let h = self.0 as HANDLE;
        let mut code = 0u32;
        unsafe {
            WaitForSingleObject(h, INFINITE);
            GetExitCodeProcess(h, &mut code);
        }
        code
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0 as HANDLE) };
    }
}

/// Starts `file` via ShellExecuteEx. `elevate` uses the "runas" verb (UAC prompt).
pub fn shell_execute(file: &Path, params: &str, dir: Option<&Path>, elevate: bool, hidden: bool) -> Result<Proc> {
    let verb = wide(if elevate { "runas" } else { "open" });
    let file_w = wide(file);
    let params_w = wide(params);
    let dir_w = dir.map(wide);
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file_w.as_ptr();
    info.lpParameters = params_w.as_ptr();
    info.lpDirectory = dir_w.as_ref().map_or(std::ptr::null(), |d| d.as_ptr());
    info.nShow = if hidden { SW_HIDE } else { SW_SHOWNORMAL };
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        let err = unsafe { GetLastError() };
        if err == ERROR_CANCELLED {
            bail!("uac_cancelled");
        }
        bail!("launch_failed", "ShellExecuteEx({}) failed: win32 error {err}", file.display());
    }
    if info.hProcess.is_null() {
        bail!("launch_failed", "no process handle for {}", file.display());
    }
    Ok(Proc(info.hProcess as usize))
}

/// Opens an http(s) URL in the default browser. The caller has already validated the URL.
pub fn open_url(url: &str) -> Result<()> {
    let verb = wide("open");
    let url_w = wide(url);
    let r = unsafe {
        ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), url_w.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL)
    };
    if (r as isize) <= 32 {
        bail!("browser_failed", "ShellExecute returned {}", r as isize);
    }
    Ok(())
}

pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
}

pub fn processes() -> Vec<ProcInfo> {
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e);
        while ok != 0 {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            out.push(ProcInfo { pid: e.th32ProcessID, name: String::from_utf16_lossy(&e.szExeFile[..len]) });
            ok = Process32NextW(snap, &mut e);
        }
        CloseHandle(snap);
    }
    out
}

/// Full image path of a process, if Windows lets us query it.
pub fn process_path(pid: u32) -> Option<PathBuf> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size);
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..size as usize])))
    }
}

/// A string value from the registry (`HKLM` when `machine`, else `HKCU`); `None` when the key or value is missing.
pub fn reg_string(machine: bool, key: &str, value: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let root = if machine { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER };
    let (key, value) = (wide(key), wide(value));
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(root, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
    };
    if r != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let s = String::from_utf16_lossy(&buf[..len]);
    (!s.is_empty()).then_some(s)
}

/// Who listens on a local IPv4 TCP port: `(pid, process name)`.
pub fn port_owner(port: u16) -> Option<(u32, String)> {
    let mut size = 0u32;
    unsafe {
        GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 0, AF_INET as u32, TCP_TABLE_OWNER_PID_LISTENER, 0);
        if size == 0 {
            return None;
        }
        // u32-aligned buffer: dwNumEntries followed by rows.
        let mut buf = vec![0u32; size as usize / 4 + 1];
        if GetExtendedTcpTable(buf.as_mut_ptr().cast(), &mut size, 0, AF_INET as u32, TCP_TABLE_OWNER_PID_LISTENER, 0) != 0 {
            return None;
        }
        let n = buf[0] as usize;
        let rows = std::slice::from_raw_parts(buf.as_ptr().add(1) as *const MIB_TCPROW_OWNER_PID, n);
        let row = rows.iter().find(|r| u16::from_be(r.dwLocalPort as u16) == port)?;
        let pid = row.dwOwningPid;
        let name = processes().into_iter().find(|p| p.pid == pid).map(|p| p.name).unwrap_or_else(|| "?".into());
        Some((pid, name))
    }
}

/// One launcher at a time (Steam "Play" while it is open would start a second relay). Returns false if another
/// instance holds the mutex; then that window is brought to the front. `wait`: retry for a few seconds (restart
/// after an update, the old process is still exiting).
pub fn single_instance(wait: bool) -> bool {
    use windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS;
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE};
    let name = wide("Local\\DungeonborneLauncher");
    for _ in 0..if wait { 50 } else { 1 } {
        let h = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
        if !h.is_null() && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
            let _held = h; // never closed: held until the process exits
            return true;
        }
        if !h.is_null() {
            unsafe { CloseHandle(h) };
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let title = wide("Dungeonborne Launcher");
    unsafe {
        let w = FindWindowW(std::ptr::null(), title.as_ptr());
        if !w.is_null() {
            ShowWindow(w, SW_RESTORE);
            SetForegroundWindow(w);
        }
    }
    false
}

pub fn port_busy_error(port: u16, e: std::io::Error) -> Error {
    match port_owner(port) {
        Some((pid, name)) => Error::new("port_busy", format!("{port}: {name} (pid {pid})")),
        None => Error::new("port_busy", format!("{port}: {e}")),
    }
}

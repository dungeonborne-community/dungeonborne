//! Win32 bits the launcher's winutil does not have: job object (kill the whole `cargo run` tree), elevation check,
//! opening a file with its associated app, hidden helper processes.

use std::path::Path;
use std::process::Command;

use std::os::windows::process::CommandExt;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use dbl::error::{Error, Result};
use dbl::winutil::wide;

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Job with KILL_ON_JOB_CLOSE: `cargo run` and the backend it starts die together, also if this app crashes.
pub struct Job(usize);

impl Job {
    pub fn new() -> Result<Self> {
        unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if h.is_null() {
                return Err(Error::new("job", "CreateJobObject failed"));
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                h,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            Ok(Self(h as usize))
        }
    }

    pub fn assign(&self, process: HANDLE) -> Result<()> {
        if unsafe { AssignProcessToJobObject(self.0 as HANDLE, process) } == 0 {
            return Err(Error::new("job", "AssignProcessToJobObject failed"));
        }
        Ok(())
    }

    pub fn terminate(&self) {
        unsafe { TerminateJobObject(self.0 as HANDLE, 1) };
    }

    pub fn contains(&self, pid: u32) -> bool {
        unsafe {
            let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if p.is_null() {
                return false;
            }
            let mut r = 0;
            let ok = IsProcessInJob(p, self.0 as HANDLE, &mut r);
            CloseHandle(p);
            ok != 0 && r != 0
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0 as HANDLE) };
    }
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut e: TOKEN_ELEVATION = std::mem::zeroed();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            (&mut e as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(token);
        ok != 0 && e.TokenIsElevated != 0
    }
}

/// Opens a file or folder with its associated program; files without an association go to Notepad.
pub fn open_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(Error::new("not_found", path.display().to_string()));
    }
    let verb = wide("open");
    let file = wide(path);
    let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    if (r as isize) > 32 {
        return Ok(());
    }
    Command::new("notepad.exe").arg(path).spawn().map(|_| ()).map_err(Error::from)
}

/// Hidden console helper (powershell, schtasks), output decoded as UTF-8 (lossy).
pub fn run_hidden(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<(i32, String)> {
    let mut cmd = Command::new(program);
    cmd.args(args).creation_flags(CREATE_NO_WINDOW);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let out = cmd.output()?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        text.push_str(&err);
    }
    Ok((out.status.code().unwrap_or(-1), text.trim().trim_start_matches('\u{feff}').to_string()))
}

/// PowerShell script with UTF-8 output; exit code of the script is returned.
pub fn powershell_file(script: &Path, args: &[&str]) -> Result<(i32, String)> {
    let quoted: Vec<String> = args.iter().map(|a| if a.starts_with('-') { a.to_string() } else { ps_quote(a) }).collect();
    let command = format!(
        "[Console]::OutputEncoding=[Text.Encoding]::UTF8; & {} {}; exit $LASTEXITCODE",
        ps_quote(&script.to_string_lossy()),
        quoted.join(" ")
    );
    run_hidden("powershell.exe", &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &command], None)
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Command line for ShellExecute: arguments with spaces or quotes get quoted (`-ABSLOG="C:\a b\x.log"` style
/// arguments are passed through as built by the caller).
pub fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|a| if (a.contains(' ') || a.is_empty()) && !a.contains('"') { format!("\"{a}\"") } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

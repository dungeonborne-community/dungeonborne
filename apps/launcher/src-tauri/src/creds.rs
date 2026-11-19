//! Session tokens live only in Windows Credential Manager (generic credentials), one per server id.

use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
use windows_sys::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};

use crate::error::{bail, Result};
use crate::winutil::wide;

fn target(server_id: &str) -> Vec<u16> {
    wide(format!("DungeonborneLauncher:{server_id}"))
}

pub fn store(server_id: &str, token: &str) -> Result<()> {
    let mut name = target(server_id);
    let mut user = wide("session");
    let mut blob = token.as_bytes().to_vec();
    let mut c: CREDENTIALW = unsafe { std::mem::zeroed() };
    c.Type = CRED_TYPE_GENERIC;
    c.TargetName = name.as_mut_ptr();
    c.UserName = user.as_mut_ptr();
    c.CredentialBlobSize = blob.len() as u32;
    c.CredentialBlob = blob.as_mut_ptr();
    c.Persist = CRED_PERSIST_LOCAL_MACHINE;
    let ok = unsafe { CredWriteW(&c, 0) };
    blob.fill(0);
    if ok == 0 {
        bail!("credentials", "CredWrite failed: win32 error {}", unsafe { GetLastError() });
    }
    Ok(())
}

pub fn load(server_id: &str) -> Result<Option<String>> {
    let name = target(server_id);
    let mut p: *mut CREDENTIALW = std::ptr::null_mut();
    let ok = unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut p) };
    if ok == 0 {
        let err = unsafe { GetLastError() };
        if err == ERROR_NOT_FOUND {
            return Ok(None);
        }
        bail!("credentials", "CredRead failed: win32 error {err}");
    }
    let token = unsafe {
        let c = &*p;
        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
        let s = String::from_utf8_lossy(bytes).into_owned();
        CredFree(p.cast());
        s
    };
    Ok(Some(token))
}

pub fn delete(server_id: &str) {
    let name = target(server_id);
    unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) };
}

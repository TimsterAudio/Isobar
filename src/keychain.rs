//! The MX password in the system's own keychain: Windows Credential Manager,
//! or the Secret Service on Linux (GNOME Keyring, KWallet). Where there's no
//! keychain, the password stays in `settings.json` as before.

use std::sync::Mutex;

const SERVICE: &str = "Isobar";
const ACCOUNT: &str = "Cumulus MX password";

/// What the keychain was last found to hold, so saving the same password
/// again doesn't ask it twice.
static KNOWN: Mutex<Option<Option<String>>> = Mutex::new(None);

#[cfg(all(any(windows, target_os = "linux"), not(test)))]
fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())
}

/// Why the keychain couldn't be used, the last time it was tried.
static PROBLEM: Mutex<Option<String>> = Mutex::new(None);

/// Where the password is kept, in words, for Settings.
pub fn place() -> String {
    match PROBLEM.lock().unwrap().clone() {
        None if cfg!(windows) => {
            "Kept in Windows Credential Manager, not in the settings file.".into()
        }
        None => "Kept in your desktop's keyring, not in the settings file.".into(),
        Some(e) => format!("Kept in the settings file: no keychain to keep it in ({e})."),
    }
}

fn note<T>(r: Result<T, String>) -> Result<T, String> {
    *PROBLEM.lock().unwrap() = r.as_ref().err().cloned();
    r
}

/// The stored password, if any. Err when there's no keychain to ask.
pub fn get() -> Result<Option<String>, String> {
    if let Some(known) = KNOWN.lock().unwrap().clone() {
        return Ok(known);
    }
    let found = note(read())?;
    *KNOWN.lock().unwrap() = Some(found.clone());
    Ok(found)
}

/// Store `password`, or remove it for None.
pub fn set(password: Option<&str>) -> Result<(), String> {
    let want = password.map(str::to_string);
    if KNOWN.lock().unwrap().as_ref() == Some(&want) {
        return Ok(());
    }
    note(write(password))?;
    *KNOWN.lock().unwrap() = Some(want);
    Ok(())
}

#[cfg(all(any(windows, target_os = "linux"), not(test)))]
fn read() -> Result<Option<String>, String> {
    match entry()?.get_password() {
        Ok(p) => Ok(Some(p)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(all(any(windows, target_os = "linux"), not(test)))]
fn write(password: Option<&str>) -> Result<(), String> {
    let e = entry()?;
    match password {
        Some(p) => e.set_password(p).map_err(|e| e.to_string()),
        None => match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        },
    }
}

// Tests, and systems with no keychain support: as if there were none.
#[cfg(any(not(any(windows, target_os = "linux")), test))]
fn read() -> Result<Option<String>, String> {
    Err(format!("no keychain for {SERVICE} {ACCOUNT}"))
}

#[cfg(any(not(any(windows, target_os = "linux")), test))]
fn write(_password: Option<&str>) -> Result<(), String> {
    Err("no keychain".into())
}

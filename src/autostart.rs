#[cfg(windows)]
use anyhow::Result;
#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const VALUE_NAME: &str = "ClaudeCron";

#[cfg(windows)]
pub fn is_enabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey(RUN_KEY) {
        Ok(key) => key.get_value::<String, _>(VALUE_NAME).is_ok(),
        Err(_) => false,
    }
}

#[cfg(windows)]
pub fn enable(exe_path: &str) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(RUN_KEY)?;
    // Wrap in quotes so paths with spaces work
    let value = format!("\"{}\"", exe_path);
    key.set_value(VALUE_NAME, &value)?;
    Ok(())
}

#[cfg(windows)]
pub fn disable() -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) {
        let _ = key.delete_value(VALUE_NAME);
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn is_enabled() -> bool { false }
#[cfg(not(windows))]
pub fn enable(_: &str) -> anyhow::Result<()> { Ok(()) }
#[cfg(not(windows))]
pub fn disable() -> anyhow::Result<()> { Ok(()) }

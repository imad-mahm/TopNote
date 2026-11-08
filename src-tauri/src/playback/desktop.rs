//! Spotify desktop: find it, start it in the background and keep its window out of sight.
//! Used when nothing is playing anywhere and TopNote's own player can't take over.
use crate::error::{msg, Result};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

/// Spotify windows TopNote hid (as raw handles), handed back when TopNote quits.
static HIDDEN: Mutex<Vec<isize>> = Mutex::new(Vec::new());

const STORE_FAMILY: &str = "SpotifyAB.SpotifyMusic_zpdnekdrzrea0";

fn classic_exe() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("APPDATA")?).join("Spotify").join("Spotify.exe");
    p.is_file().then_some(p)
}

fn store_installed() -> bool {
    std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("Packages").join(STORE_FAMILY).is_dir())
        .unwrap_or(false)
}

/// Start Spotify minimised. Its window is hidden separately by [`hide_windows`].
pub fn launch() -> Result<()> {
    if let Some(exe) = classic_exe() {
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("--minimized");
        if let Some(dir) = exe.parent() {
            cmd.current_dir(dir);
        }
        cmd.spawn()?;
        return Ok(());
    }
    if store_installed() {
        // Store apps can't take arguments this way; the window is hidden as soon as it appears.
        std::process::Command::new("explorer.exe")
            .arg(format!("shell:AppsFolder\\{STORE_FAMILY}!Spotify"))
            .spawn()?;
        return Ok(());
    }
    Err(msg("Spotify isn't installed on this PC. Install it from spotify.com and try again."))
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindow, IsWindowVisible, ShowWindow,
        GW_OWNER, SW_HIDE, SW_SHOWMINNOACTIVE,
    };

    pub fn spotify_pids() -> HashSet<u32> {
        let mut out = HashSet::new();
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
            let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            let mut ok = Process32FirstW(snap, &mut e).is_ok();
            while ok {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                if String::from_utf16_lossy(&e.szExeFile[..len]).eq_ignore_ascii_case("spotify.exe") {
                    out.insert(e.th32ProcessID);
                }
                ok = Process32NextW(snap, &mut e).is_ok();
            }
            let _ = CloseHandle(snap);
        }
        out
    }

    struct Ctx {
        pids: HashSet<u32>,
        found: Vec<isize>,
    }

    unsafe extern "system" fn collect(hwnd: HWND, lp: LPARAM) -> BOOL {
        let ctx = &mut *(lp.0 as *mut Ctx);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let top_level = GetWindow(hwnd, GW_OWNER).map(|o| o.0.is_null()).unwrap_or(true);
        if ctx.pids.contains(&pid) && top_level && IsWindowVisible(hwnd).as_bool() && GetWindowTextLengthW(hwnd) > 0 {
            ctx.found.push(hwnd.0 as isize);
        }
        BOOL(1)
    }

    /// Hide every visible Spotify main window. Returns how many were hidden.
    pub fn hide_windows() -> usize {
        let pids = spotify_pids();
        if pids.is_empty() {
            return 0;
        }
        let mut ctx = Ctx { pids, found: Vec::new() };
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut ctx as *mut Ctx as isize));
            for &h in &ctx.found {
                let _ = ShowWindow(HWND(h as *mut _), SW_HIDE);
            }
        }
        let mut hidden = HIDDEN.lock().unwrap();
        for h in &ctx.found {
            if !hidden.contains(h) {
                hidden.push(*h);
            }
        }
        ctx.found.len()
    }

    /// Give hidden Spotify windows back (minimised) so Spotify can be reached once TopNote is gone.
    pub fn restore_hidden() {
        let list = std::mem::take(&mut *HIDDEN.lock().unwrap());
        for h in list {
            let hwnd = HWND(h as *mut _);
            unsafe {
                if IsWindow(Some(hwnd)).as_bool() && !IsWindowVisible(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
                }
            }
        }
    }
}

#[cfg(windows)]
pub use imp::{hide_windows, restore_hidden, spotify_pids};

#[cfg(not(windows))]
pub fn spotify_pids() -> HashSet<u32> {
    HashSet::new()
}
#[cfg(not(windows))]
pub fn hide_windows() -> usize {
    0
}
#[cfg(not(windows))]
pub fn restore_hidden() {}

pub fn running() -> bool {
    !spotify_pids().is_empty()
}

/// Whether TopNote is currently keeping a Spotify window hidden.
pub fn has_hidden() -> bool {
    !HIDDEN.lock().unwrap().is_empty()
}

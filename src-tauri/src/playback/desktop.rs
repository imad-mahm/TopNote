//! Spotify desktop: find it, start it in the background and keep its window out of sight.
//! Used when nothing is playing anywhere and TopNote's own player can't take over.
use crate::error::{msg, Result};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

/// Spotify windows TopNote hid (as raw handles), handed back when TopNote quits.
static HIDDEN: Mutex<Vec<isize>> = Mutex::new(Vec::new());

#[cfg(not(windows))]
const NOT_INSTALLED: &str = "Spotify isn't installed on this PC. Install it (the .deb, snap or Flatpak) and try again.";
#[cfg(windows)]
const NOT_INSTALLED: &str = "Spotify isn't installed on this PC. Install it from spotify.com and try again.";

#[cfg(windows)]
const STORE_FAMILY: &str = "SpotifyAB.SpotifyMusic_zpdnekdrzrea0";

#[cfg(windows)]
fn classic_exe() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("APPDATA")?).join("Spotify").join("Spotify.exe");
    p.is_file().then_some(p)
}

#[cfg(windows)]
fn store_installed() -> bool {
    std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("Packages").join(STORE_FAMILY).is_dir())
        .unwrap_or(false)
}

/// Start Spotify minimised. Its window is hidden separately by [`hide_windows`].
#[cfg(not(windows))]
pub fn launch() -> Result<()> {
    use std::process::{Command, Stdio};
    let quiet = |mut c: Command| {
        c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        c
    };
    // deb / snap / AUR packages put a `spotify` launcher on the PATH
    if let Some(exe) = find_in_path("spotify") {
        let mut c = quiet(Command::new(exe));
        c.arg("--minimized");
        c.spawn()?;
        return Ok(());
    }
    if flatpak_installed() {
        let mut c = quiet(Command::new("flatpak"));
        c.args(["run", "com.spotify.Client", "--minimized"]);
        c.spawn()?;
        return Ok(());
    }
    Err(msg(NOT_INSTALLED))
}

#[cfg(not(windows))]
fn find_in_path(name: &str) -> Option<PathBuf> {
    let extra = ["/snap/bin", "/usr/local/bin", "/usr/bin", "/opt/spotify"];
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(extra.iter().map(PathBuf::from))
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

#[cfg(not(windows))]
fn flatpak_installed() -> bool {
    find_in_path("flatpak").is_some()
        && std::process::Command::new("flatpak")
            .args(["info", "com.spotify.Client"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

#[cfg(windows)]
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
    Err(msg(NOT_INSTALLED))
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

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, MapState, Window};
    use x11rb::rust_connection::RustConnection;

    /// Spotify's processes, from /proc (works for the deb, snap and Flatpak builds alike: sandboxed
    /// processes are still listed on the host).
    pub fn spotify_pids() -> HashSet<u32> {
        let mut out = HashSet::new();
        let Ok(dir) = std::fs::read_dir("/proc") else { return out };
        for e in dir.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
            let comm = std::fs::read_to_string(e.path().join("comm")).unwrap_or_default();
            if comm.trim().eq_ignore_ascii_case("spotify") {
                out.insert(pid);
            }
        }
        out
    }

    fn atom(c: &RustConnection, name: &str) -> Option<u32> {
        c.intern_atom(false, name.as_bytes()).ok()?.reply().ok().map(|r| r.atom)
    }

    fn client_windows(c: &RustConnection, root: Window) -> Vec<Window> {
        let Some(list) = atom(c, "_NET_CLIENT_LIST") else { return vec![] };
        c.get_property(false, root, list, AtomEnum::WINDOW, 0, 4096)
            .ok()
            .and_then(|r| r.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    fn window_pid(c: &RustConnection, w: Window, pid_atom: u32) -> Option<u32> {
        c.get_property(false, w, pid_atom, AtomEnum::CARDINAL, 0, 1)
            .ok()?
            .reply()
            .ok()?
            .value32()?
            .next()
    }

    /// Hide every visible Spotify top-level window (unmapped: gone from the screen, the taskbar
    /// and Alt-Tab, like SW_HIDE). Returns how many were hidden.
    pub fn hide_windows() -> usize {
        let pids = spotify_pids();
        if pids.is_empty() {
            return 0;
        }
        let Ok((c, screen)) = x11rb::connect(None) else { return 0 };
        let root = c.setup().roots[screen].root;
        let Some(pid_atom) = atom(&c, "_NET_WM_PID") else { return 0 };
        let mut found = Vec::new();
        for w in client_windows(&c, root) {
            let mine = window_pid(&c, w, pid_atom).is_some_and(|p| pids.contains(&p));
            let viewable = c
                .get_window_attributes(w)
                .ok()
                .and_then(|r| r.reply().ok())
                .is_some_and(|a| a.map_state == MapState::VIEWABLE);
            if mine && viewable {
                found.push(w);
            }
        }
        for &w in &found {
            let _ = c.unmap_window(w);
        }
        let _ = c.flush();
        let mut hidden = HIDDEN.lock().unwrap();
        for w in &found {
            let w = *w as isize;
            if !hidden.contains(&w) {
                hidden.push(w);
            }
        }
        found.len()
    }

    /// Give hidden Spotify windows back so Spotify can be reached once TopNote is gone.
    pub fn restore_hidden() {
        let list = std::mem::take(&mut *HIDDEN.lock().unwrap());
        if list.is_empty() {
            return;
        }
        let Ok((c, _)) = x11rb::connect(None) else { return };
        for w in list {
            let w = w as Window;
            let gone = c.get_window_attributes(w).ok().and_then(|r| r.reply().ok()).is_none();
            if !gone {
                let _ = c.map_window(w);
            }
        }
        let _ = c.flush();
    }
}

#[cfg(target_os = "linux")]
pub use imp::{hide_windows, restore_hidden, spotify_pids};

#[cfg(not(any(windows, target_os = "linux")))]
pub fn spotify_pids() -> HashSet<u32> {
    HashSet::new()
}
#[cfg(not(any(windows, target_os = "linux")))]
pub fn hide_windows() -> usize {
    0
}
#[cfg(not(any(windows, target_os = "linux")))]
pub fn restore_hidden() {}

pub fn running() -> bool {
    !spotify_pids().is_empty()
}

/// Whether TopNote is currently keeping a Spotify window hidden.
pub fn has_hidden() -> bool {
    !HIDDEN.lock().unwrap().is_empty()
}

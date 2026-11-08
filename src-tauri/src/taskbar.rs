//! Windows taskbar thumbnail toolbar: previous / play-pause / next under the hover preview.
//! All COM work happens on the main (UI) thread, which owns the window.
use crate::state::AppState;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Manager};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::{
    DefSubclassProc, ITaskbarList3, SetWindowSubclass, TaskbarList, THBF_ENABLED, THBF_HIDDEN, THBN_CLICKED, THB_FLAGS,
    THB_ICON, THB_TOOLTIP, THUMBBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIcon, GetSystemMetrics, RegisterWindowMessageW, HICON, SM_CXSMICON, WM_COMMAND,
};

const ID_PREV: u32 = 1;
const ID_TOGGLE: u32 = 2;
const ID_NEXT: u32 = 3;
const SUBCLASS_ID: usize = 0x70a7;

static APP: OnceLock<AppHandle> = OnceLock::new();
static CREATED_MSG: OnceLock<u32> = OnceLock::new();
static PLAYING: AtomicBool = AtomicBool::new(false);
static SHOWN: AtomicBool = AtomicBool::new(true);

struct Icons {
    prev: HICON,
    play: HICON,
    pause: HICON,
    next: HICON,
}

struct Tb {
    list: ITaskbarList3,
    icons: Icons,
    hwnd: HWND,
    added: bool,
}

thread_local! {
    static TB: RefCell<Option<Tb>> = const { RefCell::new(None) };
}

// ---------- tiny antialiased glyph rasterizer ----------

enum Shape {
    Tri([(f32, f32); 3]),
    Rect(f32, f32, f32, f32),
}

impl Shape {
    fn contains(&self, x: f32, y: f32) -> bool {
        match *self {
            Shape::Rect(x0, y0, x1, y1) => x >= x0 && x <= x1 && y >= y0 && y <= y1,
            Shape::Tri([a, b, c]) => {
                let s = |p: (f32, f32), q: (f32, f32)| (q.0 - p.0) * (y - p.1) - (q.1 - p.1) * (x - p.0);
                let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
                !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
            }
        }
    }
}

/// BGRA pixels of a white glyph with antialiased alpha (4x4 supersampling).
fn rasterize(shapes: &[Shape], size: usize) -> Vec<u8> {
    const S: usize = 4;
    let mut px = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let mut hits = 0;
            for sy in 0..S {
                for sx in 0..S {
                    let fx = (x as f32 + (sx as f32 + 0.5) / S as f32) / size as f32;
                    let fy = (y as f32 + (sy as f32 + 0.5) / S as f32) / size as f32;
                    if shapes.iter().any(|s| s.contains(fx, fy)) {
                        hits += 1;
                    }
                }
            }
            let i = (y * size + x) * 4;
            px[i..i + 4].copy_from_slice(&[255, 255, 255, (hits * 255 / (S * S)) as u8]);
        }
    }
    px
}

#[derive(Clone, Copy, Debug)]
enum Glyph {
    Prev,
    Play,
    Pause,
    Next,
}

fn glyph(kind: Glyph) -> Vec<Shape> {
    use Shape::*;
    match kind {
        Glyph::Prev => vec![Rect(0.22, 0.24, 0.33, 0.76), Tri([(0.78, 0.24), (0.78, 0.76), (0.33, 0.5)])],
        Glyph::Next => vec![Tri([(0.22, 0.24), (0.22, 0.76), (0.67, 0.5)]), Rect(0.67, 0.24, 0.78, 0.76)],
        Glyph::Pause => vec![Rect(0.27, 0.22, 0.42, 0.78), Rect(0.58, 0.22, 0.73, 0.78)],
        Glyph::Play => vec![Tri([(0.3, 0.2), (0.3, 0.8), (0.8, 0.5)])],
    }
}

fn make_icon(kind: Glyph) -> HICON {
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64) as usize;
    let px = rasterize(&glyph(kind), size);
    let mask = vec![0u8; size * size]; // ignored for 32-bit icons with alpha, but must be large enough
    unsafe { CreateIcon(None, size as i32, size as i32, 1, 32, mask.as_ptr(), px.as_ptr()).unwrap_or_default() }
}

fn tip(s: &str) -> [u16; 260] {
    let mut out = [0u16; 260];
    for (i, c) in s.encode_utf16().take(259).enumerate() {
        out[i] = c;
    }
    out
}

fn buttons(icons: &Icons) -> [THUMBBUTTON; 3] {
    let playing = PLAYING.load(Ordering::Relaxed);
    let flags = if SHOWN.load(Ordering::Relaxed) { THBF_ENABLED } else { THBF_HIDDEN };
    let b = |id: u32, icon: HICON, t: &str| THUMBBUTTON {
        dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
        iId: id,
        iBitmap: 0,
        hIcon: icon,
        szTip: tip(t),
        dwFlags: flags,
    };
    [
        b(ID_PREV, icons.prev, "Previous"),
        if playing { b(ID_TOGGLE, icons.pause, "Pause") } else { b(ID_TOGGLE, icons.play, "Play") },
        b(ID_NEXT, icons.next, "Next"),
    ]
}

/// (Re)attach the buttons. Called at startup and whenever Explorer recreates the taskbar button.
fn attach() {
    TB.with(|tb| {
        let mut tb = tb.borrow_mut();
        let Some(tb) = tb.as_mut() else { return };
        let btns = buttons(&tb.icons);
        unsafe {
            tb.added = tb.list.ThumbBarAddButtons(tb.hwnd, &btns).is_ok() || tb.added;
        }
    });
}

fn refresh() {
    TB.with(|tb| {
        let tb = tb.borrow();
        let Some(tb) = tb.as_ref() else { return };
        let btns = buttons(&tb.icons);
        unsafe {
            if tb.list.ThumbBarUpdateButtons(tb.hwnd, &btns).is_err() {
                let _ = tb.list.ThumbBarAddButtons(tb.hwnd, &btns);
            }
        }
    });
}

unsafe extern "system" fn subclass_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if Some(&msg) == CREATED_MSG.get() {
        TB.with(|tb| {
            if let Some(tb) = tb.borrow_mut().as_mut() {
                tb.added = false;
            }
        });
        attach();
    } else if msg == WM_COMMAND && ((wp.0 >> 16) & 0xffff) as u32 == THBN_CLICKED {
        let action = match (wp.0 & 0xffff) as u32 {
            ID_PREV => "previous",
            ID_TOGGLE => "toggle",
            ID_NEXT => "next",
            _ => "",
        };
        if let (false, Some(app)) = (action.is_empty(), APP.get()) {
            if action == "toggle" {
                // flip the icon right away; the next poll confirms it
                PLAYING.fetch_xor(true, Ordering::Relaxed);
                refresh();
            }
            let st = app.state::<Arc<AppState>>().inner().clone();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::playback::control(&app, &st, action, None).await;
            });
        }
        return LRESULT(0);
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

/// Must run on the main thread (Tauri `setup`).
pub fn install(app: &AppHandle, enabled: bool) {
    let _ = APP.set(app.clone());
    SHOWN.store(enabled, Ordering::Relaxed);
    let Some(w) = crate::window::main_window(app) else { return };
    let Ok(raw) = w.hwnd() else { return };
    let hwnd = HWND(raw.0 as _);
    unsafe {
        let _ = CREATED_MSG.set(RegisterWindowMessageW(w!("TaskbarButtonCreated")));
        let list: ITaskbarList3 = match CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) {
            Ok(l) => l,
            Err(e) => {
                log::warn!("taskbar list unavailable: {e}");
                return;
            }
        };
        if list.HrInit().is_err() {
            return;
        }
        let icons = Icons {
            prev: make_icon(Glyph::Prev),
            play: make_icon(Glyph::Play),
            pause: make_icon(Glyph::Pause),
            next: make_icon(Glyph::Next),
        };
        TB.with(|tb| *tb.borrow_mut() = Some(Tb { list, icons, hwnd, added: false }));
        let _ = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0);
    }
    attach();
}

/// Update play/pause state and visibility; cheap when nothing changed.
pub fn update(app: &AppHandle, playing: bool, enabled: bool) {
    let a = PLAYING.swap(playing, Ordering::Relaxed) != playing;
    let b = SHOWN.swap(enabled, Ordering::Relaxed) != enabled;
    if a || b {
        let _ = app.run_on_main_thread(refresh);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glyphs_have_coverage() {
        for k in [Glyph::Prev, Glyph::Play, Glyph::Pause, Glyph::Next] {
            let px = rasterize(&glyph(k), 16);
            let opaque = px.chunks(4).filter(|p| p[3] == 255).count();
            let clear = px.chunks(4).filter(|p| p[3] == 0).count();
            assert!(opaque > 10 && clear > 60, "glyph {k:?}: {opaque} opaque / {clear} clear");
        }
    }

    #[test]
    fn glyphs_are_distinct() {
        let all = [Glyph::Prev, Glyph::Play, Glyph::Pause, Glyph::Next].map(|k| rasterize(&glyph(k), 16));
        for i in 0..all.len() {
            for j in i + 1..all.len() {
                assert_ne!(all[i], all[j], "glyphs {i} and {j} look the same");
            }
        }
    }
}

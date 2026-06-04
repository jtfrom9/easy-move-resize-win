//! Easy Move+Resize (Windows port).
//!
//! A small tray utility that lets you move and resize any window by holding a
//! modifier key (default Ctrl+Alt) and dragging anywhere inside the window:
//!
//! * modifier + left-drag  -> move the window
//! * modifier + right-drag -> resize the window (which edge follows the cursor
//!   depends on which 3x3 region of the window you grabbed)
//!
//! The pointer logic lives in a global low-level mouse hook; the pure geometry
//! lives in [`geometry`] and is unit-tested independently.

#![cfg_attr(not(test), windows_subsystem = "windows")]

#[macro_use]
mod dlog_macro {
    /// `log!("fmt", ..)` — timestamped diagnostic line. Compiled in but inert
    /// unless the `EMR_LOG` environment variable is set (see [`crate::dlog`]).
    macro_rules! log {
        ($($arg:tt)*) => { $crate::dlog::write(format_args!($($arg)*)) };
    }
}

mod autostart;
mod config;
mod dlog;
mod geometry;
mod overlay;

use std::cell::Cell;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use config::Modifier;
use geometry::{drag_result, resize_edges_for_point, Rect, ResizeEdges};

/// Private window message used by the tray icon to report mouse events.
const WM_TRAY: u32 = WM_APP + 1;

// Menu command identifiers.
const ID_ENABLED: usize = 1;
const ID_BRING_FRONT: usize = 2;
const ID_AUTOSTART: usize = 3;
const ID_MOD_CTRL_ALT: usize = 10;
const ID_MOD_ALT: usize = 11;
const ID_MOD_CTRL_WIN: usize = 12;
const ID_QUIT: usize = 99;

const TIP_TEXT: &str = "Easy Move+Resize";

/// The gesture currently in progress.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Move,
    Resize,
}

/// State captured at the moment a drag begins, used to compute window geometry
/// on every subsequent mouse-move.
#[derive(Clone, Copy)]
struct Drag {
    hwnd: HWND,
    mode: Mode,
    start: POINT,
    orig: Rect,
    edges: ResizeEdges,
}

/// All mutable application state. Lives in a `thread_local` because both the
/// low-level mouse hook and the tray window procedure run on the single thread
/// that owns the message loop, so no cross-thread synchronisation is needed.
struct AppState {
    enabled: Cell<bool>,
    modifier: Cell<Modifier>,
    bring_to_front: Cell<bool>,
    drag: Cell<Option<Drag>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            enabled: Cell::new(true),
            modifier: Cell::new(Modifier::CtrlAlt),
            bring_to_front: Cell::new(true),
            drag: Cell::new(None),
        }
    }
}

thread_local! {
    static APP: AppState = AppState::new();
}

// ---------------------------------------------------------------------------
// Modifier / window helpers
// ---------------------------------------------------------------------------

/// Returns true if the given virtual key is currently pressed.
fn key_down(vk: VIRTUAL_KEY) -> bool {
    // The high-order bit of the return value is set while the key is down.
    unsafe { (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0 }
}

/// Returns true if every key of the configured modifier is held.
fn modifiers_held(m: Modifier) -> bool {
    match m {
        Modifier::CtrlAlt => key_down(VK_CONTROL) && key_down(VK_MENU),
        Modifier::Alt => key_down(VK_MENU),
        Modifier::CtrlWin => key_down(VK_CONTROL) && (key_down(VK_LWIN) || key_down(VK_RWIN)),
    }
}

/// The window class name of `hwnd` as a Rust string (empty on failure).
fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize])
}

/// Window classes we must never move/resize: the desktop and the shell's
/// taskbar(s). Dragging these around is never intended and just fights the shell.
fn is_shell_class(hwnd: HWND) -> bool {
    matches!(
        class_name(hwnd).as_str(),
        "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW"
    )
}

/// The top-level window under `pt`, or `None` if there is no manageable window
/// there (e.g. the desktop or the taskbar).
fn target_window(pt: POINT) -> Option<HWND> {
    unsafe {
        let hwnd = WindowFromPoint(pt);
        if hwnd.is_invalid() {
            return None;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        if root.is_invalid()
            || root == GetDesktopWindow()
            || root == GetShellWindow()
            || is_shell_class(root)
        {
            return None;
        }
        Some(root)
    }
}

/// Current bounds of `hwnd` as a [`Rect`], or `None` on failure.
fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r).ok()? };
    Some(Rect::from_ltrb(r.left, r.top, r.right, r.bottom))
}

/// Reposition/resize `hwnd` without changing its Z-order or activation.
fn place_window(hwnd: HWND, r: Rect) -> windows::core::Result<()> {
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            r.x,
            r.y,
            r.w,
            r.h,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSENDCHANGING,
        )
    }
}

// ---------------------------------------------------------------------------
// Drag state machine (driven by the mouse hook)
// ---------------------------------------------------------------------------

fn handle_mouse(a: &AppState, msg: u32, pt: POINT) -> bool {
    match msg {
        WM_LBUTTONDOWN => begin_drag(a, pt, Mode::Move),
        WM_RBUTTONDOWN => begin_drag(a, pt, Mode::Resize),
        // Reposition the window from the move, but NEVER swallow WM_MOUSEMOVE: a
        // low-level hook that returns non-zero for a mouse-move stops the system
        // from advancing the cursor, freezing it at the grab point (the window
        // then only jitters around its start). Always let the move pass through.
        WM_MOUSEMOVE => {
            update_drag(a, pt);
            false
        }
        WM_LBUTTONUP => end_drag(a, Mode::Move),
        WM_RBUTTONUP => end_drag(a, Mode::Resize),
        _ => false,
    }
}

/// Try to start a gesture. Returns true (swallowing the click) only if a drag
/// actually began on a manageable window.
fn begin_drag(a: &AppState, pt: POINT, mode: Mode) -> bool {
    if !a.enabled.get() || a.drag.get().is_some() || !modifiers_held(a.modifier.get()) {
        if dlog::enabled() {
            let fg = unsafe { GetForegroundWindow() };
            log!(
                "begin {:?} bail: enabled={} active={} modifier={:?} ctrl={} alt={} lwin={} rwin={} | fg-class='{}'",
                mode,
                a.enabled.get(),
                a.drag.get().is_some(),
                a.modifier.get(),
                key_down(VK_CONTROL),
                key_down(VK_MENU),
                key_down(VK_LWIN),
                key_down(VK_RWIN),
                class_name(fg)
            );
        }
        return false;
    }
    let Some(hwnd) = target_window(pt) else {
        log!("begin {:?} bail: no target window at ({},{})", mode, pt.x, pt.y);
        return false;
    };
    // Leave maximized windows alone; moving/resizing them via SetWindowPos
    // produces confusing results.
    if unsafe { IsZoomed(hwnd).as_bool() } {
        log!("begin {:?} bail: window is maximized", mode);
        return false;
    }
    let Some(orig) = window_rect(hwnd) else {
        log!("begin {:?} bail: GetWindowRect failed", mode);
        return false;
    };
    let edges = match mode {
        Mode::Resize => resize_edges_for_point(orig, pt.x, pt.y),
        Mode::Move => ResizeEdges::default(),
    };
    if a.bring_to_front.get() {
        // Raise in Z-order without stealing keyboard focus, so moving a
        // background window doesn't redirect the user's typing.
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
    if dlog::enabled() {
        log!(
            "begin {:?} START hwnd={:#x} class='{}' orig=({},{},{},{}) start=({},{}) edges={:?}",
            mode,
            hwnd.0 as usize,
            class_name(hwnd),
            orig.x,
            orig.y,
            orig.w,
            orig.h,
            pt.x,
            pt.y,
            edges
        );
    }
    a.drag.set(Some(Drag {
        hwnd,
        mode,
        start: pt,
        orig,
        edges,
    }));
    overlay::show(orig);
    true
}

/// Apply the current cursor position to the in-progress drag.
fn update_drag(a: &AppState, pt: POINT) -> bool {
    let Some(d) = a.drag.get() else {
        return false;
    };
    // If the target window vanished mid-drag (e.g. the app closed it), abandon
    // the gesture so we stop acting on a stale handle and swallowing input.
    if !unsafe { IsWindow(d.hwnd).as_bool() } {
        a.drag.set(None);
        overlay::hide();
        return false;
    }
    let dx = pt.x - d.start.x;
    let dy = pt.y - d.start.y;
    let r = drag_result(d.orig, d.edges, d.mode == Mode::Resize, dx, dy);
    if let Err(e) = place_window(d.hwnd, r) {
        log!(
            "update {:?} pt=({},{}) d=({},{}) -> ({},{},{},{}) setpos=ERR {}",
            d.mode, pt.x, pt.y, dx, dy, r.x, r.y, r.w, r.h, e
        );
    }
    overlay::update(r);
    true
}

/// End a drag if the released button matches the active gesture.
fn end_drag(a: &AppState, mode: Mode) -> bool {
    match a.drag.get() {
        Some(d) if d.mode == mode => {
            log!("end {:?}", mode);
            a.drag.set(None);
            overlay::hide();
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Win32 callbacks
// ---------------------------------------------------------------------------

/// Low-level mouse hook. Runs on the message-loop thread for every mouse event
/// system-wide; consumes only the button down/up that start and end a gesture,
/// never mouse-moves (swallowing those would freeze the cursor).
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wparam.0 as u32;
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let handled = APP.with(|a| handle_mouse(a, msg, info.pt));
        if handled {
            return LRESULT(1);
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

/// Window procedure for the hidden message-only window that hosts the tray icon.
unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY => {
            let event = (lparam.0 as u32) & 0xFFFF;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                show_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ---------------------------------------------------------------------------
// Tray menu
// ---------------------------------------------------------------------------

unsafe fn append(menu: HMENU, id: usize, text: &str, checked: bool) {
    let mut flags = MF_STRING;
    if checked {
        flags |= MF_CHECKED;
    }
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let _ = AppendMenuW(menu, flags, id, PCWSTR(wide.as_ptr()));
}

unsafe fn append_separator(menu: HMENU) {
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
}

/// Build and display the tray context menu at the cursor, then act on the
/// chosen command.
fn show_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        APP.with(|a| {
            append(menu, ID_ENABLED, "Enabled", a.enabled.get());
            append(
                menu,
                ID_BRING_FRONT,
                "Bring window to front",
                a.bring_to_front.get(),
            );
            append(menu, ID_AUTOSTART, "Start at login", autostart::is_enabled());
            append_separator(menu);
            let m = a.modifier.get();
            append(menu, ID_MOD_CTRL_ALT, "Modifier: Ctrl + Alt", m == Modifier::CtrlAlt);
            append(menu, ID_MOD_ALT, "Modifier: Alt", m == Modifier::Alt);
            append(menu, ID_MOD_CTRL_WIN, "Modifier: Ctrl + Win", m == Modifier::CtrlWin);
            append_separator(menu);
            append(menu, ID_QUIT, "Quit", false);
        });

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Required so the menu dismisses correctly when the user clicks away.
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD,
            pt.x,
            pt.y,
            0,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        handle_command(hwnd, cmd.0 as usize);
    }
}

fn handle_command(hwnd: HWND, cmd: usize) {
    APP.with(|a| match cmd {
        ID_ENABLED => a.enabled.set(!a.enabled.get()),
        ID_BRING_FRONT => a.bring_to_front.set(!a.bring_to_front.get()),
        ID_AUTOSTART => autostart::toggle(),
        ID_MOD_CTRL_ALT => a.modifier.set(Modifier::CtrlAlt),
        ID_MOD_ALT => a.modifier.set(Modifier::Alt),
        ID_MOD_CTRL_WIN => a.modifier.set(Modifier::CtrlWin),
        ID_QUIT => unsafe {
            let _ = DestroyWindow(hwnd);
        },
        _ => {}
    });
}

// ---------------------------------------------------------------------------
// Setup / message loop
// ---------------------------------------------------------------------------

/// The embedded application icon (resource id 1), falling back to the default
/// system icon if the resource isn't present.
unsafe fn load_app_icon() -> HICON {
    let hinst = HINSTANCE(GetModuleHandleW(None).unwrap_or_default().0);
    // MAKEINTRESOURCE(1): the resource name is the integer id 1, encoded as a
    // pointer-sized address (not a real pointer that gets dereferenced).
    let id = PCWSTR(std::ptr::without_provenance(1));
    LoadIconW(hinst, id)
        .or_else(|_| LoadIconW(None, IDI_APPLICATION))
        .unwrap_or_default()
}

fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        ..Default::default()
    };
    nid.hIcon = unsafe { load_app_icon() };
    for (i, c) in TIP_TEXT.encode_utf16().enumerate() {
        if i >= nid.szTip.len() - 1 {
            break;
        }
        nid.szTip[i] = c;
    }
    nid
}

fn main() -> windows::core::Result<()> {
    dlog::init();
    unsafe {
        // Operate in physical pixels so hook coordinates match SetWindowPos.
        let dpi = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        log!("startup: SetProcessDpiAwarenessContext(PER_MONITOR_V2) = {:?}", dpi);

        let hinstance = HINSTANCE(GetModuleHandleW(None)?.0);

        let class_name = w!("EasyMoveResizeWnd");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("Easy Move+Resize"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            hinstance,
            None,
        )?;

        overlay::init(hinstance);

        let nid = tray_data(hwnd);
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);

        // WH_MOUSE_LL is a global hook whose procedure lives in this EXE; the
        // documented hmod for a low-level hook is NULL.
        let _hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), HINSTANCE::default(), 0)?;
        log!("startup: WH_MOUSE_LL hook installed; entering message loop");

        // Standard message loop. GetMessageW returns 0 on WM_QUIT and -1 on
        // error; either way we stop and tear the tray icon down.
        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
    Ok(())
}

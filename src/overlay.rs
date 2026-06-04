//! A click-through silhouette drawn over the window being dragged.
//!
//! One layered, transparent, top-most, non-activating pop-up window is created
//! at startup and kept hidden. During a gesture it covers the target window and
//! is filled with a translucent accent colour (a silhouette), with the live
//! size and position printed in the centre. `WS_EX_TRANSPARENT` makes it ignore
//! the mouse, so it never interferes with the hook or the app underneath.

use std::cell::Cell;

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DrawTextW, EndPaint, InvalidateRect, SelectObject,
    SetBkMode, SetTextColor, DT_CALCRECT, DT_CENTER, DT_NOCLIP, HFONT, HGDIOBJ, PAINTSTRUCT,
    TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::geometry::Rect;

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

/// Silhouette fill colour and the centred text colour.
const ACCENT: COLORREF = rgb(45, 140, 240);
const WHITE: COLORREF = rgb(255, 255, 255);
/// Whole-window opacity (0..255). High enough to read clearly as a silhouette.
const ALPHA: u8 = 180;

thread_local! {
    static OVERLAY: Cell<Option<HWND>> = const { Cell::new(None) };
    static FONT: Cell<Option<HFONT>> = const { Cell::new(None) };
    static CURRENT: Cell<Rect> = const { Cell::new(Rect { x: 0, y: 0, w: 0, h: 0 }) };
}

unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // Belt-and-suspenders click-through (on top of WS_EX_TRANSPARENT).
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Draw the centred "W x H / (X, Y)" label. The accent fill is supplied by the
/// class background brush during erase.
unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);

    let r = CURRENT.with(|c| c.get());
    let label = format!("{} \u{00d7} {}\n({}, {})", r.w, r.h, r.x, r.y);
    let mut text: Vec<u16> = label.encode_utf16().collect();

    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);

    FONT.with(|f| {
        if let Some(font) = f.get() {
            SelectObject(hdc, HGDIOBJ(font.0));
        }
    });
    let _ = SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, WHITE);

    // Vertically centre the two-line label within the window.
    let mut calc = client;
    DrawTextW(hdc, &mut text, &mut calc, DT_CENTER | DT_CALCRECT | DT_NOCLIP);
    let text_h = calc.bottom - calc.top;
    let mut draw = client;
    draw.top += ((client.bottom - client.top - text_h) / 2).max(0);
    DrawTextW(hdc, &mut text, &mut draw, DT_CENTER | DT_NOCLIP);

    let _ = EndPaint(hwnd, &ps);
}

/// Create the hidden overlay window. Call once at startup.
pub fn init(hinstance: HINSTANCE) {
    unsafe {
        let class = w!("EmrOverlayWnd");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(overlay_proc),
            hInstance: hinstance,
            lpszClassName: class,
            hbrBackground: CreateSolidBrush(ACCENT),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let ex = WS_EX_LAYERED
            | WS_EX_TRANSPARENT
            | WS_EX_TOOLWINDOW
            | WS_EX_NOACTIVATE
            | WS_EX_TOPMOST;
        if let Ok(hwnd) = CreateWindowExW(
            ex, class, w!("emr-overlay"), WS_POPUP, 0, 0, 0, 0, None, None, hinstance, None,
        ) {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), ALPHA, LWA_ALPHA);
            // Bold ~22px label.
            let font = CreateFontW(-30, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 0, 0, w!("Segoe UI"));
            OVERLAY.with(|o| o.set(Some(hwnd)));
            FONT.with(|f| f.set(Some(font)));
        }
    }
}

/// Show the silhouette over `target`.
pub fn show(target: Rect) {
    place(target, true);
}

/// Move/resize the silhouette to follow `target`.
pub fn update(target: Rect) {
    place(target, false);
}

fn place(target: Rect, first: bool) {
    OVERLAY.with(|o| {
        if let Some(hwnd) = o.get() {
            CURRENT.with(|c| c.set(target));
            unsafe {
                let (after, flags) = if first {
                    (HWND_TOPMOST, SWP_NOACTIVATE | SWP_SHOWWINDOW)
                } else {
                    (HWND::default(), SWP_NOACTIVATE | SWP_NOZORDER)
                };
                let _ = SetWindowPos(hwnd, after, target.x, target.y, target.w, target.h, flags);
                let _ = InvalidateRect(hwnd, None, true);
            }
        }
    });
}

/// Hide the silhouette.
pub fn hide() {
    OVERLAY.with(|o| {
        if let Some(hwnd) = o.get() {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    });
}

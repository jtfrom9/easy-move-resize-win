//! User-configurable settings.

/// Modifier key combination that must be held to start a move/resize gesture.
///
/// Windows has no Cmd key, so the macOS "Ctrl+Cmd" default is mapped to one of
/// these Windows-native combinations. All listed keys must be held together.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Modifier {
    /// Ctrl + Alt (default).
    CtrlAlt,
    /// Alt only (classic X11 convention).
    Alt,
    /// Ctrl + Win (closest literal mapping of macOS Ctrl+Cmd).
    CtrlWin,
}

//! Embed the application icon (and basic version info) into the Windows PE so
//! the executable and its tray icon use our custom artwork.

fn main() {
    println!("cargo:rerun-if-changed=assets/easy-move-resize.ico");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        // Resource id "1": the lowest-id group icon, which Explorer shows for
        // the exe and which we load at runtime for the tray icon.
        res.set_icon_with_id("assets/easy-move-resize.ico", "1");
        res.set("ProductName", "Easy Move+Resize");
        res.set(
            "FileDescription",
            "Move and resize windows by dragging with a modifier key",
        );
        if let Err(e) = res.compile() {
            // Don't fail the build if the resource compiler is unavailable; the
            // app falls back to the default system icon.
            println!("cargo:warning=icon embedding skipped: {e}");
        }
    }
}

//! Alidade — iced shell on Kaliber tokens (plan 052, UI Task 1).
//!
//! This binary boots the window and loads the Inter font family. Everything
//! else lives in the library target (`lib.rs`) so integration tests can
//! reach it as `alidade_app::…`.

use alidade_app::state::App;
use alidade_app::ui::font_regular;

const FONT_REGULAR_BYTES: &[u8] = include_bytes!("../vendor/fonts/Inter-Regular.ttf");
const FONT_MEDIUM_BYTES: &[u8] = include_bytes!("../vendor/fonts/Inter-Medium.ttf");
const FONT_SEMIBOLD_BYTES: &[u8] = include_bytes!("../vendor/fonts/Inter-SemiBold.ttf");

const WINDOW_W: f32 = 1280.0;
const WINDOW_H: f32 = 800.0;

/// `--minimized` starts the window hidden (Task 6 Step 2), for an autostart
/// entry that should not pop a window on login.
const MINIMIZED_FLAG: &str = "--minimized";

fn main() -> iced::Result {
    let start_minimized = std::env::args().any(|arg| arg == MINIMIZED_FLAG);

    iced::application(move || App::boot(start_minimized), App::update, App::view)
        .title("Alidade")
        .window_size((WINDOW_W, WINDOW_H))
        // The window hides to the tray on close instead of exiting outright
        // (Task 6 Step 2); `App::update`'s `WindowCloseRequested` handler
        // decides between hiding and a real close.
        .exit_on_close_request(false)
        .subscription(App::subscription)
        .font(FONT_REGULAR_BYTES)
        .font(FONT_MEDIUM_BYTES)
        .font(FONT_SEMIBOLD_BYTES)
        .default_font(font_regular())
        .run()
}

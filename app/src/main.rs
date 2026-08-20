//! Alidade — iced shell on Kaliber tokens (plan 052, UI Task 1).
//!
//! This binary boots the window and loads the Inter font family. Everything
//! else lives in the library target (`lib.rs`) so integration tests can
//! reach it as `alidade_app::…`.

use alidade_app::state::App;
use alidade_app::ui::font_regular;

const FONT_REGULAR_BYTES: &[u8] = include_bytes!("../../../../design/assets/fonts/Inter-Regular.ttf");
const FONT_MEDIUM_BYTES: &[u8] = include_bytes!("../../../../design/assets/fonts/Inter-Medium.ttf");
const FONT_SEMIBOLD_BYTES: &[u8] = include_bytes!("../../../../design/assets/fonts/Inter-SemiBold.ttf");

const WINDOW_W: f32 = 1280.0;
const WINDOW_H: f32 = 800.0;

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Alidade")
        .window_size((WINDOW_W, WINDOW_H))
        .font(FONT_REGULAR_BYTES)
        .font(FONT_MEDIUM_BYTES)
        .font(FONT_SEMIBOLD_BYTES)
        .default_font(font_regular())
        .run()
}

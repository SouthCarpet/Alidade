//! Alidade — iced shell on Kaliber tokens (plan 052, UI Task 1).
//!
//! This binary boots the window and loads the Fira Sans font families. Everything
//! else lives in the library target (`lib.rs`) so integration tests can
//! reach it as `alidade_app::…`.

use alidade_app::startup::{self, StartupFlags};
use alidade_app::state::App;
use alidade_app::ui::font_regular;

const FONT_SANS_REGULAR_BYTES: &[u8] =
    include_bytes!("../vendor/assets/fonts/fira-sans/FiraSans-Regular.ttf");
const FONT_SANS_MEDIUM_BYTES: &[u8] =
    include_bytes!("../vendor/assets/fonts/fira-sans/FiraSans-Medium.ttf");
const FONT_SANS_SEMIBOLD_BYTES: &[u8] =
    include_bytes!("../vendor/assets/fonts/fira-sans/FiraSans-SemiBold.ttf");
const FONT_SANS_BOLD_BYTES: &[u8] =
    include_bytes!("../vendor/assets/fonts/fira-sans/FiraSans-Bold.ttf");
const FONT_TABULAR_REGULAR_BYTES: &[u8] =
    include_bytes!("../vendor/assets/fonts/fira-sans/FiraSansTabular-Regular.ttf");
const FONT_TABULAR_MEDIUM_BYTES: &[u8] =
    include_bytes!("../vendor/assets/fonts/fira-sans/FiraSansTabular-Medium.ttf");
const FONT_TABULAR_SEMIBOLD_BYTES: &[u8] =
    include_bytes!("../vendor/assets/fonts/fira-sans/FiraSansTabular-SemiBold.ttf");
const FONT_TABULAR_BOLD_BYTES: &[u8] =
    include_bytes!("../vendor/assets/fonts/fira-sans/FiraSansTabular-Bold.ttf");

fn main() -> iced::Result {
    let flags: StartupFlags = startup::parse(std::env::args().skip(1));

    // `--db` (Task 7 pre-work B1) is applied as an environment variable
    // before boot rather than threaded through `App`: `default_db_path`
    // already reads one (`ALIDADE_DB_PATH`), so this is the one place that
    // decides where the store opens, for every code path that calls it.
    if let Some(db) = &flags.db {
        std::env::set_var("ALIDADE_DB_PATH", db);
    }

    let window = flags.window;
    let boot_flags = flags.clone();

    iced::application(
        move || App::boot(boot_flags.clone()),
        App::update,
        App::view,
    )
    .title("Alidade")
    .window_size(window)
    // The window hides to the tray on close instead of exiting outright
    // (Task 6 Step 2); `App::update`'s `WindowCloseRequested` handler
    // decides between hiding and a real close.
    .exit_on_close_request(false)
    .subscription(App::subscription)
    .font(FONT_SANS_REGULAR_BYTES)
    .font(FONT_SANS_MEDIUM_BYTES)
    .font(FONT_SANS_SEMIBOLD_BYTES)
    .font(FONT_SANS_BOLD_BYTES)
    .font(FONT_TABULAR_REGULAR_BYTES)
    .font(FONT_TABULAR_MEDIUM_BYTES)
    .font(FONT_TABULAR_SEMIBOLD_BYTES)
    .font(FONT_TABULAR_BOLD_BYTES)
    .default_font(font_regular())
    .run()
}

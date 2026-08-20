//! Screen modules. Each one owns a `view(app) -> Element<Message>` function
//! (plan Task 2+); `ui.rs` routes to the active screen's module.

pub mod home;

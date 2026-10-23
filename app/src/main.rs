//! Runs the app on the machine's own backend.
//!
//! macOS: `cargo run` opens the window; for live edits run it through the
//! daemon (`cargo ra-jit run`, then `cargo ra-jit build` after each edit —
//! `../dev.sh` watches and does that). iOS: this binary is the `.app`'s
//! executable; `../ios.sh` builds it for the simulator, packs the bundle
//! and launches it.

#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;

/// The root the runners mount: the same component every platform shows.
pub fn app_root() -> nsh_core::ui::Root {
    nsh_core::ui::Root::of::<stack_native_app::App>().app_id("ai.stacknative.app")
}

/// The UI sets the engine serves: nsh's base components and the app's.
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn ui_sets() -> Vec<nsh_core::ui::Registry> {
    vec![nsh_base::registry(), stack_native_app::registry()]
}

#[cfg(target_os = "macos")]
fn main() -> impl std::process::Termination {
    env_logger::init();
    platform::set_ui_sets(ui_sets());
    platform::run_app(
        platform::AppSource::Rust(app_root()),
        platform::AppOptions {
            title: "Stack Native".to_owned(),
            app_id: "ai.stacknative.app".to_owned(),
            size: (1100, 760),
            ..Default::default()
        },
    )
}

#[cfg(target_os = "ios")]
fn main() {
    nsh_platform_ios::set_ui_sets(ui_sets());
    nsh_ios_app::run_rust(app_root());
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn main() {
    eprintln!("stack-native-app runs on macOS (`cargo run`) and iOS (packaged by ../ios.sh)");
    std::process::exit(1);
}

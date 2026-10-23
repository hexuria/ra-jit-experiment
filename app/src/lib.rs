//! Stack Native: an all-Rust app on nsh, edited live by ra-jit.
//!
//! The root is a thin shell ([`App`]): a `dynamic` mounts [`hot::HotArea`]
//! — which the daemon's patches cause to rebuild — over a status line that
//! says what each patch did and shows the app never stopped (the uptime
//! keeps counting across remounts).
//!
//! [`live::install`] is the only glue between ra-jit and the engine: it
//! polls the patch file on the engine thread, snapshots the `keep` signals
//! and bumps `remount`, which the dynamic reads. Everything below it runs
//! again, on new code, with its kept state.

mod hot;
pub mod live;

use std::time::Duration;

use nsh_core::ui::prelude::*;

/// As in `hot.rs`: colors built of plain `Hsla` literals — `to_hsla` /
/// `clamp`/`max` in const eval are not in ra-jit's evaluator yet, and a
/// function whose consts cannot be evaluated goes to cargo, not a patch.
const fn tone(light: Hsla, dark: Hsla) -> AdaptiveColor {
    AdaptiveColor { light, dark }
}
const fn hsla(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    Hsla { h, s, l, a }
}

#[cfg(target_os = "ios")]
const SHOW_NOTE: bool = false;
#[cfg(not(target_os = "ios"))]
const SHOW_NOTE: bool = true;

/// Extra lift for the status bar so it clears the iPhone home indicator;
/// desktop has no unsafe bottom edge.
#[cfg(target_os = "ios")]
const BAR_LIFT: f32 = 22.0;
#[cfg(not(target_os = "ios"))]
const BAR_LIFT: f32 = 0.0;

const BG: AdaptiveColor = tone(hsla(0.17, 0.10, 0.95, 1.0), hsla(0.60, 0.11, 0.07, 1.0));
const BAR: AdaptiveColor = tone(hsla(0.15, 0.05, 0.92, 1.0), hsla(0.60, 0.07, 0.10, 1.0));
const BORDER: AdaptiveColor = tone(hsla(0.15, 0.07, 0.86, 1.0), hsla(0.60, 0.08, 0.19, 1.0));
const TEXT: AdaptiveColor = tone(hsla(0.60, 0.10, 0.09, 1.0), hsla(0.58, 0.24, 0.94, 1.0));
const MUTED: AdaptiveColor = tone(hsla(0.60, 0.06, 0.40, 1.0), hsla(0.60, 0.07, 0.59, 1.0));
const UP: AdaptiveColor = tone(hsla(0.42, 0.75, 0.31, 1.0), hsla(0.42, 0.67, 0.60, 1.0));

/// The app's UI set for nsh tooling (`nsh.json` `"rust"`).
#[native_module("stack-native-app/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// The shell: the hot area in a `dynamic` that rebuilds on each patch,
/// over a status line. This view runs once, when the app starts — its
/// signals and its interval therefore outlive every remount, which is
/// what makes the status line a witness that the process never stopped.
#[component]
pub fn App() -> impl IntoElement {
    let remount = signal(0u64);
    let note = signal("ra-jit-live linked; run `cargo ra-jit build` after an edit".to_owned());
    live::install(remount, note);
    let seconds = signal(0u64);
    set_interval(Duration::from_secs(1), move || seconds.update(|s| *s += 1));
    live::start_game();

    view()
        .flex_1()
        .h_full()
        .bg(BG)
        .child(
            view().flex_1().child(each(
                move || vec![remount.get()],
                |&n| n,
                move |_, _| hot::hot_area(),
            )),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(16.0))
                .px_4()
                .py_2()
                .pb(px(8.0 + BAR_LIFT))
                .bg(BAR)
                .border_t_1()
                .border_t_color(BORDER)
                .child(
                    text(move || {
                        if SHOW_NOTE {
                            note.get()
                        } else {
                            String::new()
                        }
                    })
                    .flex_1()
                    .text_size(px(12.0))
                    .font_family("monospace")
                    .text_color(TEXT),
                )
                .child(
                    text(move || format!("remounts {}", remount.get()))
                        .text_size(px(12.0))
                        .font_family("monospace")
                        .text_color(UP),
                )
                .child(
                    text(move || format!("up {}s", seconds.get()))
                        .text_size(px(12.0))
                        .font_family("monospace")
                        .text_color(MUTED),
                ),
        )
}

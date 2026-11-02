//! Optional host-window full screen (HarmonyOS system bars hidden). The host injects
//! [`WindowFullscreen`] through `Services`; without it nothing here is shown, so every other
//! platform is unchanged. The host owns the real window state: [`request`] only asks, and the host
//! reports the outcome (also when the system leaves full screen by itself) through `is_active`.

use crate::{PhotocraftApp, i18n};

/// Control-command id that toggles host full screen from a host or automation. The menu entry
/// point is Photoshop's View › Screen Mode (`F`), which routes here when a host offers it.
pub const COMMAND: &str = "window.fullscreen.toggle";

pub struct WindowFullscreen {
    /// Whether the host window is currently full screen (read every frame; the host requests a
    /// repaint itself when the state changes).
    pub is_active: Box<dyn Fn() -> bool>,
    /// Ask the host to enter (`true`) or leave (`false`) full screen.
    pub request: Box<dyn Fn(bool)>,
}

/// Whether the host offers full screen at all.
pub fn available(app: &PhotocraftApp) -> bool {
    app.services.window_fullscreen.is_some()
}

/// The host window's current state; `false` without a host service.
pub fn active(app: &PhotocraftApp) -> bool {
    app.services.window_fullscreen.as_ref().is_some_and(|f| (f.is_active)())
}

/// English label of the action the next click performs (translated when drawn).
pub fn label(active: bool) -> &'static str {
    if active { "Exit Full Screen" } else { "Enter Full Screen" }
}

/// Toggle: ask the host for the opposite of its current state.
pub fn toggle(app: &PhotocraftApp) -> bool {
    let Some(f) = app.services.window_fullscreen.as_ref() else { return false };
    let target = !(f.is_active)();
    (f.request)(target);
    target
}

/// The translated label, for tooltips and menu text.
pub fn tr_label(active: bool) -> String {
    i18n::tr(i18n::current(), label(active)).to_owned()
}

/// Follow the state the host reports, without asking it again: the system can leave full screen on
/// its own (a gesture, a resize out of full screen) or refuse the request. Screen Mode is where the
/// chrome reads it from, so both stay in step.
pub fn adopt(app: &mut PhotocraftApp, ctx: &egui::Context, on: bool) {
    let full = app.ui.view.screen_mode != "standard";
    if full == on {
        return;
    }
    app.ui.view.screen_mode = if on { "fullScreen" } else { "standard" }.to_owned();
    // Tab's view of the chrome lasts only while in Full Screen Mode, as on the desktop.
    app.ui.panels.full_screen_chrome = false;
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn labels_follow_the_next_action() {
        assert_eq!(label(false), "Enter Full Screen");
        assert_eq!(label(true), "Exit Full Screen");
        // The label is translated for the interface language (English when a catalogue lacks it).
        assert_eq!(tr_label(false), i18n::tr(i18n::current(), "Enter Full Screen"));
    }

    #[test]
    fn toggle_requests_the_opposite_of_the_host_state() {
        let state = Rc::new(Cell::new(false));
        let asked = Rc::new(Cell::new(None));
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        assert!(!available(&app) && !toggle(&app));
        let (s, a) = (state.clone(), asked.clone());
        app.services.window_fullscreen = Some(WindowFullscreen { is_active: Box::new(move || s.get()), request: Box::new(move |on| a.set(Some(on))) });
        assert!(available(&app) && !active(&app));
        assert!(toggle(&app));
        assert_eq!(asked.get(), Some(true));
        state.set(true);
        assert!(active(&app) && !toggle(&app));
        assert_eq!(asked.get(), Some(false));
    }

    #[test]
    fn adopt_mirrors_the_reported_state_without_asking_the_host() {
        let asked = Rc::new(Cell::new(0));
        let a = asked.clone();
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", serde_json::json!({"width": 32, "height": 32})).unwrap();
        app.services.window_fullscreen =
            Some(WindowFullscreen { is_active: Box::new(|| false), request: Box::new(move |_| a.set(a.get() + 1)) });
        let ctx = egui::Context::default();
        assert_eq!(app.ui.view.screen_mode, "standard");
        // The system left full screen by itself: Screen Mode follows, and nothing is requested.
        adopt(&mut app, &ctx, true);
        assert_eq!(app.ui.view.screen_mode, "fullScreen");
        adopt(&mut app, &ctx, false);
        assert_eq!(app.ui.view.screen_mode, "standard");
        assert_eq!(asked.get(), 0);
    }
}

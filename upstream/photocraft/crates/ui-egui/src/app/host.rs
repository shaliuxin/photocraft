//! The frame a host that is not eframe drives.
//!
//! On the desktop eframe owns the window: its event loop delivers the input, and each pass is
//! handed an `eframe::Frame` carrying the window and its timings. A platform shell that owns the
//! window itself — HarmonyOS' ArkUI/XComponent, a scripted host — drives the same three stages
//! directly and passes no frame at all: [`file_dialog::ParentFrame`] is `()` there.
//!
//! One frame, in the window's order: [`PhotocraftApp::host_input`], then
//! [`PhotocraftApp::host_logic`], then [`PhotocraftApp::host_ui`]. Keep `host_ui` last, and call
//! it exactly once per frame that ran `host_input`: it closes the frame's stage record.

use super::{PhotocraftApp, gpu_canvas, shortcuts, stylus, titlebar, widgets, wheel_nav};

impl PhotocraftApp {
    /// Input: the events the host collected since the previous frame, adjusted in place.
    ///
    /// Everything here rewrites the events the widgets will see, so it must run before any
    /// widget: the host's own title-bar handling, then the shortcuts that turn clipboard keys
    /// into text, then the events the control channel injected.
    pub fn host_input(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let frame_start = gpu_canvas::now_ms();
        // An undecorated window gets its caption back when the toolkit changes its flags
        // (maximize, restore, …): take it off again before the frame is laid out (#2246).
        if let Some(window_frame) = self.services.window_frame.as_mut() {
            window_frame();
        }
        // Native menu key equivalents become the key presses they were (see `native_menu`).
        if let Some(menu) = self.services.native_menu.as_mut() {
            menu.raw_input(raw_input);
        }
        // While a text field of a floating panel's own window has the keyboard, the dock area
        // hands that window the text keys in this input and converts them there.
        if craft_ui::docking::typing_window(ctx).is_none() {
            shortcuts::clipboard_keys(ctx, ctx.text_edit_focused() || self.ui.text_edit.is_some(), raw_input);
        }
        // Windows precision touchpads and some Linux Wayland compositors send pinches
        // as fractional Ctrl + wheel rather than a native Zoom event (#4566).
        // Leave Ctrl+whole-notch mouse scrolling alone, and honour the pinch preference.
        if cfg!(any(target_os = "windows", target_os = "linux")) && self.session.prefs().enhanced_controls.zoom_with_trackpad_pinch {
            wheel_nav::fold_legacy_pinch(ctx, raw_input);
        }
        self.inject_synthetic(ctx, raw_input);
        // After the control channel's own events (`ui.type`) join the OS's, before any widget.
        widgets::cap_text_events(&mut raw_input.events);
        self.control_click.normalize(&mut raw_input.events, cfg!(target_os = "macos"));
        // A pen held still is no touch screen's press-and-hold right click (stylus.rs).
        let pen_down = self.stylus.pen_contacts.observe(&raw_input.events);
        stylus::pen_hold_is_no_right_click(ctx, pen_down);
        if self.custom_titlebar {
            titlebar::release_after_os_resize(ctx, raw_input);
        }
        // The OS's moves and the control channel's (injected just above).
        let moves = raw_input.events.iter().filter(|e| matches!(e, egui::Event::PointerMoved(_))).count();
        self.perf.stage.start(frame_start, moves);
    }

    /// Logic: the frame's work before anything is drawn (commands, jobs, documents, prefs).
    pub fn host_logic(&mut self, ctx: &egui::Context) {
        self.logic_frame(ctx, None);
        self.finish_type_reset_logic(ctx);
    }

    /// The UI pass: draw the editor into a full-window `Ui` and close the frame's stage record.
    pub fn host_ui(&mut self, ui: &mut egui::Ui) {
        self.ui_frame(ui, None);
        self.finish_type_reset_pass(ui.ctx());
    }

    /// Closing the window: keep what the app remembers across runs and answer pending captures.
    /// A host calls this once, after its last frame; the desktop reaches it through `on_exit`.
    pub fn host_exit(&mut self) {
        if let Err(err) = super::tool_memory::save_on_exit(self) {
            log::warn!("Preferences: could not save the final tool selection: {err}");
        }
        // Release a pending screenshot's image budget and answer its control request; the
        // callback is inert afterwards (it holds only a weak slot).
        let finished = self.captures.cancel_all("the application is closing");
        self.finish_captures(finished);
    }
}

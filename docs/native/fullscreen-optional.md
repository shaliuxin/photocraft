# Optional window full screen

Default behaviour is **unchanged**: the window keeps its status bar and navigation indicator, `SystemBars.ets` keeps colouring them with the theme, and the page lays out inside the safe area. Full screen is something the user switches on.

## Entry points (egui, HarmonyOS only)

| Where | What |
| --- | --- |
| View › Screen Mode (`F` cycles) | Photoshop's own Screen Mode entry point. When the host installs `Services::window_fullscreen`, the three modes collapse to the window's two states: *Standard* asks for `false`, both full-screen modes ask for `true`. The menu stays in step with the window through `window_fullscreen::adopt`. |
| Control command `window.fullscreen.toggle` | For a host or automation: asks for the opposite of the window's state and returns `{"fullScreen": bool}`. |

Both exist only when the host installs `Services::window_fullscreen` (`upstream/.../window_fullscreen.rs`); every other platform keeps the viewport commands and the tests see no change.

The v0.16 fork's status-bar icon button (and the **Enter Full Screen / Exit Full Screen** Window-menu item under *Theme*) is **not** carried over: v0.18's icon set has no full-screen glyph, and View › Screen Mode already is the desktop-reachable path. Add the button back only with real art, not invented icon names.

## Flow

1. egui: `Screen Mode` / `window.fullscreen.toggle` → `WindowFullscreen::request` → `bridge::notify(0, "fullscreen", "enter" | "exit")`.
2. `Index.ets` → `requestFullscreen()` (`Fullscreen.ets`): `setWindowLayoutFullScreen(true)` + `setWindowSystemBarEnable([])`; exit restores `setWindowLayoutFullScreen(false)` + `['status', 'navigation']` and repaints the theme colour through `SystemBars.ets`.
3. ArkTS → Rust `native.input({"kind":"fullscreen_state","text":"on"|"off"})` after **every** request (also on failure or refusal). Rust mirrors this state (`adopt` writes it back to Screen Mode), so the chrome and the menu never stay ahead of the window when the system refuses or leaves full screen by itself.
4. `FullscreenSync` (EntryAbility) listens to `windowStatusChange`: if the system turns the window into a floating/split window while full screen is on, it leaves full screen and reports `off`. On foreground it re-hides the bars if full screen is on.

Full screen draws edge to edge: no inset padding is applied. `setKeyboardAvoidMode(RESIZE)` is untouched.

## 2in1

Not offered: `fullscreenAvailable()` is false, ArkTS sends `fullscreen:false` in the native config, Rust installs no service, and Screen Mode keeps the plain viewport behaviour (no full-screen mode to enter). Freeform windows share the title strip with `WindowChrome`; immersive layout would fight it. Floating and split windows on tablets refuse to enter full screen for the same reason.

## Risk

While full screen, strokes that start at the very bottom (or top) edge can be taken by the system swipe gestures (home / recents / notification shade). Leave full screen or start a little inside the canvas.

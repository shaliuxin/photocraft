//! Device form factor reported by the ArkTS host.
//!
//! One HAP now ships for phones, tablets and 2in1 PCs (`deviceTypes` in module.json5) and on two
//! ABIs, so nothing in the native host may assume a desktop-shaped window. ArkTS passes
//! `deviceType` and the editor width in vp with the rest of the launch config; this module turns
//! them into the single value layout decisions read, instead of scattering `deviceInfo` checks
//! through the adapter.
//!
//! The thresholds follow ArkUI's own breakpoints: a window narrower than 600 vp is the compact
//! column layout, 600–840 vp is medium, and wider is the desktop layout ArkUI calls expanded.

/// Layout class of the window the editor is drawing into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum WidthClass {
    /// Narrower than 600 vp: a phone held upright, or a 2in1 window dragged small.
    Compact,
    /// 600–840 vp: a phone in landscape, or a small tablet window.
    Medium,
    /// Wider than 840 vp: a tablet or a 2in1 window.
    Expanded,
}

/// How the host should lay the editor out.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct Form {
    /// Value of `deviceInfo.deviceType`; empty when ArkTS did not report one.
    pub device_type: FormFactor,
    /// Editor width in vp; zero when unknown.
    pub width_vp: f32,
    pub width_class: WidthClass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FormFactor {
    Phone,
    Tablet,
    Desktop,
    /// Missing or unrecognised `deviceType`; fall back to the width class alone.
    Unknown,
}

impl Form {
    /// Width-only classification, used when `deviceType` is absent and by tests.
    pub fn from_width(width_vp: f32) -> WidthClass {
        if !width_vp.is_finite() || width_vp <= 0. {
            WidthClass::Expanded
        } else if width_vp < 600. {
            WidthClass::Compact
        } else if width_vp < 840. {
            WidthClass::Medium
        } else {
            WidthClass::Expanded
        }
    }

    pub fn new(device_type: &str, width_vp: f32) -> Self {
        let factor = match device_type {
            "phone" => FormFactor::Phone,
            "tablet" => FormFactor::Tablet,
            "2in1" | "pc" | "tv" => FormFactor::Desktop,
            _ => FormFactor::Unknown,
        };
        // A phone in landscape reports a wide window, so the device type wins over the width: the
        // phone column layout is what the user expects, not a rotated desktop.
        let width_class = match factor {
            FormFactor::Phone => WidthClass::Compact,
            _ => Self::from_width(width_vp),
        };
        Self { device_type: factor, width_vp, width_class }
    }

    /// True when the editor should use the compact column layout.
    #[allow(dead_code)]
    pub fn compact(&self) -> bool {
        self.width_class == WidthClass::Compact
    }
}

#[cfg(test)]
#[path = "../tests/unit/form_factor.rs"]
mod tests;

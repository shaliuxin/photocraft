use super::*;

#[test]
fn width_classes_follow_the_arkui_breakpoints() {
    assert_eq!(Form::from_width(360.), WidthClass::Compact, "a phone column layout");
    assert_eq!(Form::from_width(599.), WidthClass::Compact);
    assert_eq!(Form::from_width(600.), WidthClass::Medium);
    assert_eq!(Form::from_width(839.), WidthClass::Medium);
    assert_eq!(Form::from_width(840.), WidthClass::Expanded);
    assert_eq!(Form::from_width(1440.), WidthClass::Expanded);
}

#[test]
fn a_missing_width_does_not_force_the_compact_layout() {
    assert_eq!(Form::from_width(0.), WidthClass::Expanded);
    assert_eq!(Form::from_width(f32::NAN), WidthClass::Expanded);
}

#[test]
fn the_device_type_wins_over_a_wide_window_on_a_phone() {
    // A phone in landscape reports a wide window; the column layout is still what it needs.
    let form = Form::new("phone", 900.);
    assert_eq!(form.device_type, FormFactor::Phone);
    assert_eq!(form.width_class, WidthClass::Compact);
    assert!(form.compact());
}

#[test]
fn tablets_and_2in1_follow_the_window_width() {
    assert_eq!(Form::new("tablet", 1280.).width_class, WidthClass::Expanded);
    assert_eq!(Form::new("2in1", 640.).width_class, WidthClass::Medium);
    // An unknown or unreported device type falls back to the width alone.
    assert_eq!(Form::new("", 380.).width_class, WidthClass::Compact);
    assert_eq!(Form::new("wearable", 380.).device_type, FormFactor::Unknown);
}

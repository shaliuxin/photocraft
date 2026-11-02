//! 手写笔接入层的单测：线格式、信号源、屏幕轴倾斜。
//!
//! 上游 0.18 的笔状态机有自己的测试（`crates/ui-egui/src/input/stylus/tests.rs`），
//! 这里只验证「鸿蒙上报的字节」如何变成上游认得的样本。

use super::*;

/// ArkTS 侧 `StylusProtocol.ets` 真实产出的包；`scripts/test_stylus_adapter.cjs` 用同一份夹具。
fn fixture() -> Vec<StylusPacket> {
    let packets: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../fixtures/stylus-packets.json")).unwrap();
    packets
        .into_iter()
        .map(|value| serde_json::from_value::<StylusPacket>(value).unwrap())
        .collect()
}

#[test]
fn arkts_packets_parse_with_camel_case_fields() {
    let packets = fixture();
    assert_eq!(packets.len(), 7);
    assert!(matches!(
        &packets[0].input,
        StylusInput::Connection {
            connected: true,
            ..
        }
    ));
    match &packets[1].input {
        StylusInput::Pointer { source, sample } => {
            assert_eq!(*source, PointerSource::Pen);
            let sample = sample.as_ref().unwrap();
            assert!((sample.pressure - 0.25).abs() < f32::EPSILON);
            assert!((sample.tilt_x - 30.).abs() < f32::EPSILON);
            assert!((sample.tilt_y + 20.).abs() < f32::EPSILON);
            assert!((sample.rotation - 90.).abs() < f32::EPSILON);
            assert!(!sample.eraser);
        }
        other => assert!(false, "第二个包应当是 pointer：{other:?}"),
    }
    let mut touch = false;
    for packet in &packets[2..] {
        match &packet.input {
            StylusInput::BarrelButton { .. } | StylusInput::Gesture { .. } => {}
            StylusInput::Pointer {
                source: PointerSource::Touch,
                sample: None,
            } => touch = true,
            other => assert!(false, "夹具里没有这种包：{other:?}"),
        }
    }
    assert!(touch, "最后一包是手指抬起，样本为空");
}

#[test]
fn malformed_packets_are_rejected_by_the_wire_types() {
    for json in [
        r#"{"type":"pointer","source":"pen","sample":null}"#,
        r#"{"version":1,"type":"unknown"}"#,
        r#"{"version":1,"type":"pointer","source":"unknown","sample":null}"#,
        r#"{"version":1,"type":"gesture","gesture":"unknownGesture","timestampMs":1}"#,
    ] {
        assert!(
            serde_json::from_str::<StylusPacket>(json).is_err(),
            "{json}"
        );
    }
    // 版本号能解析但不一样：由调用方整包丢弃，不在这里猜语义。
    let newer: StylusPacket =
        serde_json::from_str(r#"{"version":2,"type":"pointer","source":"pen","sample":null}"#)
            .unwrap();
    assert_ne!(newer.version, VERSION);
}

#[test]
fn pen_readings_reach_the_upstream_feed_but_mouse_and_fingers_clear_it() {
    let feed = StylusFeed::default();
    let mut pen = PenLink::new(feed.clone());
    pen.report(
        PointerSource::Pen,
        Some(PenReading {
            pressure: 0.4,
            tilt_x: 12.0,
            ..Default::default()
        }),
    );
    let sample = feed.get().unwrap();
    assert!((sample.pressure - 0.4).abs() < f32::EPSILON);
    assert!((sample.tilt_x - 12.0).abs() < f32::EPSILON);

    // 手指和鼠标必须把笔样本清掉，否则鼠标会沿用手写笔的压力继续画。
    pen.report(PointerSource::Touch, None);
    assert_eq!(pen.source(), PointerSource::Touch);
    assert!(feed.get().is_none());
    pen.report(PointerSource::Pen, None);
    assert!(feed.get().is_none(), "带 source=pen 但没有读数不产生样本");
}

#[test]
fn out_of_range_and_non_finite_readings_are_sanitised() {
    let feed = StylusFeed::default();
    let mut pen = PenLink::new(feed.clone());
    pen.report(
        PointerSource::Pen,
        Some(PenReading {
            pressure: 7.5,
            tilt_x: f32::NAN,
            tilt_y: -400.0,
            rotation: -10.0,
            eraser: true,
        }),
    );
    let sample = feed.get().unwrap();
    assert!((sample.pressure - 1.0).abs() < f32::EPSILON);
    assert_eq!(sample.tilt_x, 0.0);
    assert!((sample.tilt_y + 90.0).abs() < f32::EPSILON);
    assert!((sample.rotation - 350.0).abs() < f32::EPSILON);
    assert!(sample.eraser);
}

fn at(tilt_x: f32, tilt_y: f32) -> PenSample {
    PenSample {
        pressure: 0.5,
        tilt_x,
        tilt_y,
        rotation: 0.0,
        eraser: false,
    }
}

#[test]
fn one_quarter_turn_swaps_the_tilt_axes() {
    let sample = at(45.0, 0.0);
    let turned = tilt_in_screen_axes(sample, 1);
    assert!(turned.tilt_x.abs() < 0.001);
    assert!((turned.tilt_y + 45.0).abs() < 0.001);
    let twice = tilt_in_screen_axes(turned, 1);
    assert!((twice.tilt_x + 45.0).abs() < 0.001);
    assert!(twice.tilt_y.abs() < 0.001);
    // 四个四分圈回到原样。
    assert_eq!(tilt_in_screen_axes(sample, 4), sample);
    assert_eq!(tilt_in_screen_axes(sample, 0), sample);
}

#[test]
fn landscape_correction_is_two_quarter_turns() {
    let feed = StylusFeed::default();
    let mut pen = PenLink::new(feed.clone());
    pen.set_display_rotation(0, true);
    pen.report(
        PointerSource::Pen,
        Some(PenReading {
            pressure: 0.5,
            tilt_x: 20.0,
            tilt_y: 10.0,
            ..Default::default()
        }),
    );
    let held = feed.get().unwrap();
    let expected = tilt_in_screen_axes(at(20.0, 10.0), 2);
    assert!((held.tilt_x - expected.tilt_x).abs() < 0.001);
    assert!((held.tilt_y - expected.tilt_y).abs() < 0.001);
}

#[test]
fn rotation_reorients_the_held_sample_so_a_still_pen_does_not_keep_the_old_axis() {
    let feed = StylusFeed::default();
    let mut pen = PenLink::new(feed.clone());
    pen.report(
        PointerSource::Pen,
        Some(PenReading {
            pressure: 0.5,
            tilt_x: 45.0,
            ..Default::default()
        }),
    );
    assert!(feed.get().unwrap().tilt_y.abs() < 0.001);
    pen.set_display_rotation(1, false);
    let held = feed.get().unwrap();
    assert!(held.tilt_x.abs() < 0.001);
    assert!((held.tilt_y + 45.0).abs() < 0.001);
    // 越界的象限不接受，样本保持原样。
    pen.set_display_rotation(9, false);
    assert!((feed.get().unwrap().tilt_y + 45.0).abs() < 0.001);
}

#[test]
fn only_pointer_packets_ask_for_a_repaint() {
    let feed = StylusFeed::default();
    let mut pen = PenLink::new(feed.clone());
    for packet in fixture() {
        let is_pointer = matches!(&packet.input, StylusInput::Pointer { .. });
        assert_eq!(pen.submit(packet.input), is_pointer);
    }
    // 最后一包是手指抬起，样本被清掉。
    assert!(feed.get().is_none());
}

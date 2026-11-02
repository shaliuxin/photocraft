//! 鸿蒙手写笔接入层：把 ArkTS / C++ 上报的笔读数送进上游 0.18 的 `app.stylus.feed`。
//!
//! v0.16 的移植自带一套 `StylusFeed`（信号源、悬停/接触诊断、手势队列、倾角校正都长在里面），
//! 而 0.18 上游的 `input::stylus` 已经是唯一的笔状态机（`Stylus`、`PenContacts`、`strokes`、
//! 压力曲线）。两套状态机不能并存，所以本文件只保留上游没有的三件事：
//!
//! 1. **线格式**：ArkTS 与 C++ 用 camelCase 的 `type` 标签包上报，上游的类型不带 serde；
//! 2. **屏幕轴倾斜**：XComponent 给的倾角以面板自然方向为基准，屏幕转过去后要跟着转；
//! 3. **信号源**：上游只看样本，不看「这一帧是谁在指」，宿主需要它决定笔预览是否隐藏。
//!
//! 投递走的是上游给平台后端留的公开入口 [`StylusFeed::set`]——Web runner、macOS 平板
//! 监视器、X11 读值器都走同一条路，因此上游后续改笔算法不需要动这里。

use photocraft_ui_egui::stylus::{PenSample, StylusFeed};
use serde::Deserialize;

/// 线格式里的一笔读数。缺省压力按 1.0，与上游 `PenSample::default()` 一致。
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PenReading {
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub rotation: f32,
    pub eraser: bool,
}

impl Default for PenReading {
    fn default() -> Self {
        Self {
            pressure: 1.0,
            tilt_x: 0.0,
            tilt_y: 0.0,
            rotation: 0.0,
            eraser: false,
        }
    }
}

impl PenReading {
    /// 换成上游的样本类型；`sanitized` 负责夹到文档范围并替换非有限值。
    pub fn sample(self) -> PenSample {
        PenSample {
            pressure: self.pressure,
            tilt_x: self.tilt_x,
            tilt_y: self.tilt_y,
            rotation: self.rotation,
            eraser: self.eraser,
        }
        .sanitized()
    }
}

/// 这一帧的指针归谁。上游 0.18 不再有这个名字，但宿主必须区分：手指按下时要藏笔预览。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PointerSource {
    #[default]
    Mouse,
    Pen,
    Touch,
}

/// 笔身按键动作。ArkTS 侧仍会上报，0.18 已没有消费方（v0.16 的标题栏笔菜单未移植）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PenGesture {
    DoubleTap,
    #[serde(alias = "longPress")]
    Squeeze,
}

/// 平台无关的笔输入。字段名与 `entry/src/main/ets/stylus/StylusProtocol.ets` 逐字对应。
#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StylusInput {
    Pointer {
        source: PointerSource,
        sample: Option<PenReading>,
    },
    /// 保留解析：让上报按键的宿主不被整包拒收，目前无消费方。
    #[allow(dead_code)]
    BarrelButton { pressed: bool, timestamp_ms: f64 },
    /// 保留解析：同上。
    #[allow(dead_code)]
    Gesture {
        gesture: PenGesture,
        timestamp_ms: f64,
    },
    #[allow(dead_code)]
    Connection {
        connected: bool,
        device_name: Option<String>,
    },
}

/// 带版本号的一包笔事件。
#[derive(Clone, Debug, Deserialize)]
pub struct StylusPacket {
    pub version: u32,
    #[serde(flatten)]
    pub input: StylusInput,
}

/// 当前线格式版本。ArkTS 的 `STYLUS_PROTOCOL_VERSION` 必须与它相等：版本不认识就整包丢弃，
/// 不猜它的语义。
pub const VERSION: u32 = 1;

/// 一个视图的笔入口。`feed` 是 `app.stylus.feed` 的克隆，写进去的样本由上游自己消费。
#[derive(Clone, Debug, Default)]
pub struct PenLink {
    feed: StylusFeed,
    source: PointerSource,
    rotation: u8,
    landscape: bool,
}

impl PenLink {
    pub fn new(feed: StylusFeed) -> Self {
        Self {
            feed,
            ..Default::default()
        }
    }

    /// 最近一次上报的指针类型。
    pub fn source(&self) -> PointerSource {
        self.source
    }

    /// 当前笔样本（`None` 表示没有笔在按）。只给单测用：生产路径由上游自己读队列。
    #[cfg(test)]
    pub fn sample(&self) -> Option<PenSample> {
        self.feed.get()
    }

    /// 记下这一帧的指针类型，并把读数投给上游。
    ///
    /// 只有「手写笔 + 带读数」才产生样本：鼠标和手指必须把样本清掉，否则笔离开屏幕后
    /// 鼠标会沿用手写笔的压力继续画。
    pub fn report(&mut self, source: PointerSource, reading: Option<PenReading>) {
        self.source = source;
        let sample = match (source, reading) {
            (PointerSource::Pen, Some(reading)) => {
                Some(tilt_in_screen_axes(reading.sample(), self.tilt()))
            }
            _ => None,
        };
        self.feed.set(sample);
    }

    /// 一次带版本的包。返回是否需要重绘：只有 `pointer` 包真的改了这一帧的指针状态。
    pub fn submit(&mut self, input: StylusInput) -> bool {
        match input {
            StylusInput::Pointer { source, sample } => {
                self.report(source, sample);
                true
            }
            // 下面三类都能解析，但 0.18 的笔逻辑不消费它们：按键手势在 v0.16 只用于
            // 标题栏笔菜单，菜单没随 0.18 移植过来。这里如实返回「不重绘」。
            StylusInput::BarrelButton { .. }
            | StylusInput::Gesture { .. }
            | StylusInput::Connection { .. } => false,
        }
    }

    /// 显示方向：顺时针四分圈（0..=3）＋当前是否横屏。
    pub fn set_display_rotation(&mut self, quarter_turns: u8, landscape: bool) {
        if quarter_turns > 3 {
            return;
        }
        let previous = self.tilt();
        self.rotation = quarter_turns;
        self.landscape = landscape;
        let delta = (self.tilt() + 4 - previous) % 4;
        // 笔停住不动时上游仍会读上一笔样本；屏幕转了方向就把这笔样本也转过去，
        // 否则静止的笔会沿用旧轴直到下一次移动。
        if delta != 0
            && let Some(sample) = self.feed.get()
        {
            self.feed.set(Some(tilt_in_screen_axes(sample, delta)));
        }
    }

    /// 横屏时面板的自然方向比屏幕多转两圈，倾角要跟着补回来。
    fn tilt(&self) -> u8 {
        if self.landscape {
            self.rotation.wrapping_add(2) % 4
        } else {
            self.rotation % 4
        }
    }
}

/// 把硬件倾角换到当前屏幕轴。XComponent 报告的倾角以面板自然方向为基准，
/// 屏幕绕它转，所以把倾角矢量反向转回去即可。
fn tilt_in_screen_axes(sample: PenSample, quarter_turns: u8) -> PenSample {
    if quarter_turns.is_multiple_of(4) {
        return sample;
    }
    // tan() 是相对竖直方向的倾角正切；转动它的 x/y 分量等于把笔绕 z 轴转，倾斜幅度不变。
    let x = sample.tilt_x.clamp(-89.999, 89.999).to_radians().tan();
    let y = sample.tilt_y.clamp(-89.999, 89.999).to_radians().tan();
    let (x, y) = match quarter_turns % 4 {
        1 => (y, -x),
        2 => (-x, -y),
        _ => (-y, x),
    };
    PenSample {
        tilt_x: x.atan().to_degrees(),
        tilt_y: y.atan().to_degrees(),
        ..sample
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "../tests/unit/stylus.rs"]
mod tests;

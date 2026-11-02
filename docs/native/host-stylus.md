# 手写笔接入层（v0.18）

鸿蒙上报的笔读数怎么进到画布：`native/rust/src/stylus.rs` 加 `input.rs` 的指针状态机，
一共 3 件事。压力曲线、笔迹插值、橡皮端切换全部由上游 0.18 的笔逻辑负责，适配层不参与。

## 一、为什么不再搬 `StylusFeed`

v0.16 的移植在 `ui-egui` 里自己实现了一套 `StylusFeed`（信号源、悬停/接触诊断、手势队列、
倾角校正都长在这一个类型里）。0.18 上游也有一个 `StylusFeed`，但它只是个样本队列，笔状态机
是 `Stylus`（`input/stylus.rs`，用到 `PenContacts`、`strokes`、压力曲线、橡皮端）。

两套状态机拼在一起只有两种结果：要么适配层的样本被上游覆盖，要么上游的插值只看到最后一笔。
所以适配层不再实现队列，只按上游给平台后端留的公开入口投递：

```rust
app.stylus.feed.set(Some(PenSample { pressure, tilt_x, tilt_y, rotation, eraser }));
```

Web runner、macOS 平板监视器、X11 读值器走的都是这一条路。上游以后重写笔算法，这里不用改。

## 二、三层职责

| 职责 | 位置 | 说明 |
|---|---|---|
| 线格式 | `stylus.rs` 的 `StylusInput` / `StylusPacket` | 与 `entry/src/main/ets/stylus/StylusProtocol.ets` 逐字对应，camelCase，`type` 标签分派 |
| 屏幕轴倾斜 | `stylus.rs` 的 `tilt_in_screen_axes` | XComponent 的倾角以面板自然方向为基准，屏幕转过去后要把倾角矢量转回来 |
| 信号源 | `stylus.rs` 的 `PenLink` | 上游只看样本、不看「谁在指」，宿主需要它决定笔预览是否隐藏 |

`PenLink` 是唯一有状态的类型：一个视图一个，`feed` 是 `app.stylus.feed` 的克隆，
`source` 与 `rotation` 跟着这个视图走。子窗口在 `viewports.rs` 里各自 `PenLink::new(feed)`。

## 三、线格式

```json
{"kind":"stylus","packet":{"version":1,"type":"pointer","source":"pen",
 "sample":{"pressure":0.25,"tiltX":30,"tiltY":-20,"rotation":90,"eraser":false}}}
```

- `version` 不认识就整包丢弃（`input.rs` 里对着 `stylus::VERSION` 比）。宁可没有笔手势，
  也不要按错的语义执行。
- `type` 有四种：`pointer`、`barrelButton`、`gesture`、`connection`。后三种能解析但**没有
  消费方**——见第五节。
- `sample` 缺失或为 `null` 表示「笔还在，但这一帧没有读数」；`source` 不是 `pen` 时适配层
  会清掉样本，否则鼠标会沿用手写笔的压力继续画。
- 读数先过上游的 `PenSample::sanitized()`：压力夹到 0..1，倾角夹到 ±90，旋转取
  `rem_euclid(360)`，非有限值换成默认值。

夹具 `native/rust/tests/fixtures/stylus-packets.json` 是唯一契约：ArkTS 工厂
（`scripts/test_stylus_adapter.cjs`）和 Rust 解析（`tests/unit/stylus.rs`）都对着它比。

## 四、倾角换到屏幕轴

面板的自然方向是固定的，屏幕绕它转 `rotation` 个四分圈（顺时针），横屏时再差两圈：

```
tilt = (rotation + if landscape { 2 } else { 0 }) % 4
```

换算是把倾角的正切当二维矢量转：

```
x = tan(tilt_x), y = tan(tilt_y)
转一圈: (x, y) -> (y, -x)
```

屏幕方向变化时，笔可能静止不动。上游会一直读上一笔样本，所以 `set_display_rotation`
要把已投递的样本也转过去，否则静止的笔一直用旧轴直到下一次移动。

## 五、0.18 里没有对应物的东西

移植时如实丢掉，不留假实现：

| v0.16 的行为 | 0.18 现状 |
|---|---|
| 双击笔身 / 长按笔身触发的标题栏笔菜单 | 没有移植标题栏笔菜单（`chrome_ui` 未随 0.18 过来），`barrelButton` / `gesture` 只解析不执行 |
| `source()` 区分笔/手指后隐藏笔轮廓 | 0.18 的 `canvas.rs` 没有这条规则，属于第 3 层（canvas 230 行改动）未完成的部分 |
| `stylus_debug` 调试窗口（样本率、悬停点、双击计数） | 0.18 没有该窗口；诊断字段随 `StylusFeed` 一起删除 |
| `StylusDevice`（连接状态与设备名） | 0.18 没有消费方，`connection` 包只解析 |

这四项要回来，得按上游的形态重新设计（笔菜单进菜单栏、轮廓规则进 canvas 补丁），
不是在适配层恢复一个字段就能了事。

## 六、验证

```
cd native/rust
cargo check --target aarch64-unknown-linux-ohos          # 0 错误
cargo check --target x86_64-unknown-linux-ohos           # 0 错误
cargo check --tests --target aarch64-unknown-linux-ohos   # 0 错误（含单测）
```

单测覆盖：夹具解析、坏包拒收、样本投递与清理、越界读数清洗、四个四分圈的倾角换算、
横屏补半圈、旋转时移动中的样本、只有 `pointer` 包请求重绘。
**这四条命令只证明能编译**；真机行为要在鸿蒙设备上跑，见 `pinch-finger-rejection.md`、
`touch-cancellation.md`、`touchscreen-navigation.md` 的验收步骤。

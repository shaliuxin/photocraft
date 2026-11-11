# 上游补丁系列

`upstream/photocraft/` 里的树是 **纯净上游 v0.22.0 + 这里按序号应用的补丁**。补丁只放「上游必须知道的那点事」，鸿蒙自己的代码在 `native/rust/`（适配层）和 `entry/`（ArkTS 侧），不进补丁。

| 文件 | 内容 | 规模 |
|---|---|---|
| `0001-host-frame.patch` | 宿主扩展点：`eframe` 可选、`host_input`/`host_logic`/`host_ui`/`host_exit`、`file_dialog::ParentFrame`、`self-update` 特性、直接依赖 `egui-wgpu`/`wgpu`；携带 4 个宿主/外观**新文件** `app/host.rs`、`path_pick.rs`、`path_pick_tests.rs`（166 行单测）、`window_fullscreen.rs`；把 v0.21.0 新增的 `canvas/gpu_canvas/compositor_build.rs` 从 `eframe::wgpu::*` 改成直接 `wgpu::*`（无 eframe 的宿主构建才编得过）；`app.rs` 的 eframe impl 继续委托 `host_input` 并保留上游在 `ui()` 的像素缓存/spill 完整性检查，退出流程继续委托 `host_exit()`；**v0.22.0 起 `updates_ui.rs` 的整份自更新逻辑统一套上 `feature = "self-update"`**（上游新增 `requested`/`last_update`/`check_now` 后，裸 `not(wasm32)` 门控会让 `craft_update` 泄漏进 `default-features = false` 的宿主构建） | 862 增 / 101 删 / 19 文件 |
| `0002-engine-hooks.patch` | 引擎钩子：`installed_profiles::set_app_profile_dir`、`format::atomic::set_host_writer`、`text::cjk::set_ui_locale`、`i18n::set_host_locale`、ArkUI 主题枚举、`prefs/validate.rs` 的 OHOS 门控；另修两条 CJK 测试断言——`tests/it/system_locale.rs`（File 标题经 `menu_keys::localized` 带助记符后缀如"文件(F)"，原测试精确查裸"文件"）与 v0.21.0 新增的 `tests/it/crop_overlay_icons.rs`（画布菜单行经 `tl!` 翻译，原测试查英文原名 `"Rule of Thirds"`，改为查 `i18n::t(kind.label())`）。**v0.21.0 起手写笔偏好采用上游的 `TabletInput`/`use_tablet_pressure`**，宿主旧加的 `StylusDoubleTap`/`StylusLongPress`/`block_finger_input*` 已移除（全树无消费代码） | 341 增 / 25 删 / 9 文件 |
| `0003-host-fonts.patch` | 宿主字体：`text/cjk_fonts.rs` 的 `font_bytes_ok`、`store_host_cjk_font`、`reapply_host_fonts` | 77 增 / 1 文件 |
| `0004-chrome-sandbox-seams.patch` | 沙箱与外观：`Services` 的 `pick_open`/`pick_import`/`pick_save`/`pick_folder`/`sandbox_dir`/`documents_dir`/`path_pickers_only`/`always_pick_save`/`default_save`、`ask_file` 同步直答、`window_fullscreen`、`host_titlebar`、`native_doc_windows`、`ThemeKind::chrome_css`；宿主全屏经 View › Screen Mode（`F`）请求 `Services::window_fullscreen`（不发 eframe 的视口命令），宿主回报的状态再用 `adopt` 映回 Screen Mode | 135 增 / 1 删 / 10 文件 |
| `0005-ohos-memory.patch` | **OHOS 可用内存探测（2026-11-11 真机修复）**：`raster/src/memory/system.rs` 在 `target_env = "ohos"` 下改走 `/proc/meminfo` 的 `MemAvailable`（保守上限 2 GiB、swap 记 0、`constraint_unreadable = false`），并把通用 Linux cgroup v1/v2 模块从 OHOS 构建中排除。OHOS 把 memory 控制器挂在 `/dev/memcg`、mountinfo 不带控制器名且应用沙箱不可读，通用探测器会 fail-closed 返回 `Some(0)`，使 `io::memory::available()` 为 0、导入预算 soft/hard 都为 0，任何普通照片都报「内存不足」；另加 2 条平台无关单测 | 85 增 / 5 删 / 1 文件 |

合计 1,500 增 / 132 删，**40 个文件**。v0.16 的分叉是 7,620 行 / 114 个文件。

**分区规则**：v0.22.0 重生成时把每个文件的完整差异只放进**一个**补丁（按它在旧系列里首次出现的补丁归属），五个补丁合起来正好是「纯净 v0.22.0 → 仓库树」的全量差异，互不重叠。所以补丁的 a 侧 blob 就是 v0.22.0 本身，在纯净树上直接 `git apply` 即可干净应用，不需要靠 v0.21.0 的旧 blob 做 3-way。`0005` 是升级后追加的独立修复，同样以 v0.22.0 的该文件为 a 侧，不与前四个补丁重叠。

五个文件都是纯 `git diff --full-index` 输出（`a/upstream/photocraft/...` 前缀，在仓库根应用），不含提交信息；每个补丁为什么存在，看这张表。

**手写笔没有补丁**：0.18 的 `app.stylus.feed` 就是平台后端的投递入口，鸿蒙侧只做线格式与屏幕轴倾角，见 `docs/native/host-stylus.md`。

## v0.21.0 → v0.22.0 的冲突处理

仍用三方合并（`base` = 纯净 v0.21.0，`ours` = 纯净 v0.22.0，`theirs` = 已验证的 v0.21.0 宿主树）重放宿主改动：33 个文件自动合并、4 个新文件直接带过来，**只有 2 个文件 / 2 处**需人工取舍：

- `ui-egui/src/app.rs`：上游 `on_exit` 里新增了 `captures.cancel_all` / `finish_captures`。保留宿主的单行 `self.host_exit();`——`app/host.rs` 里的 `host_exit()` 本来就把 save on exit、取消截图、收尾回答三件事都做了，不丢上游功能。
- `ui-egui/src/updates_ui.rs`：v0.22.0 把自更新扩充成带 `requested`/`last_update`/`check_now`/`dismiss_status` 的状态机。正确解法不是二选一，而是**取上游新逻辑 + 把宿主的 feature 门控变换统一重套到整个文件**：所有自更新块的 `not(wasm32)` 改成 `all(not(wasm32), feature = "self-update")`，wasm 兜底改成 `any(wasm32, not(feature = "self-update"))`。最终 0 个裸 `not(wasm32)` 残留。`Services.last_update` 是无门控的普通字段（类型不引用 `craft_update`），保持上游原样即可。

依赖层零漂移：egui 0.37.0 / wgpu 30.0.1 / winit 0.30.13 全部不变，`wgpu-hal-30.0.1` 无需重 vendor；新增的只是上游 `[patch.crates-io]` 里 4 行 `hayro-*`，由 `pins --write` 自动带入。

重生成后已在纯净 v0.22.0 上按序 `git apply` 4 个补丁，重放树 3,141 个文件与仓库树**逐字节一致**。（`0005` 为 2026-11-11 追加的第 5 个补丁，重放时按序号接在后面即可。）

## v0.20.1 → v0.21.0 的冲突处理

用三方合并（`base` = 纯净 v0.20.1，`ours` = 纯净 v0.21.0，`theirs` = 已验证的 v0.20.1 宿主树）重放宿主改动，29 个文件自动合并、4 个新文件直接带过来，只有 4 个文件 / 7 处需人工取舍：

- `engine/src/prefs.rs`（3 处）、`prefs/validate.rs`（2 处）：上游把宿主自加的手写笔偏好整块换成 `TabletInput`。宿主树里没有任何消费者，故采用上游版；ArkUI 主题与 OHOS 门控等宿主改动不受影响。
- `ui-egui/src/app.rs`：保留宿主把输入委托给 `host_input` 的单一实现，把上游新增的 `tablet_poll` 挪进 `host_input`；`ui()` 保留上游新增的缓存/spill 检查，参数仍用宿主的 `Some(frame)`。
- `ui-egui/src/app/frame.rs`：保留上游新增的 `home_screen_up()`、`memory_warning::tick`、`queue_web_places()`，签名仍用宿主的 `Option<&ParentFrame>`；`monitor_status::poll_frame` 与 `poll_file_dialog` 继续收 `parent`。

合并结果已用 `cargo check -p photocraft-ui-egui`（默认特性）与 `--no-default-features`（宿主路径）双双验过。

## 为什么是补丁而不是分叉

v0.16 的做法是把上游整份拷下来改，改了 114 个文件、7620 行。上游 v0.16→v0.18 有 58 个文件冲突、72 个冲突块，`ui-egui` 还被重排成目录结构，14 个改过的文件直接变成 `modify/delete`——单 `lib.rs` 重放 54 个补丁块有 47 个失败。

同样的升级，补丁系列只需重放一个 150 行的补丁。

## 应用与重放

当前树已经应用过，正常构建不需要再跑。换上游版本时见 `docs/native/upstream-sync.md` 的「replaying the patch series」：把新版本解到 `upstream/photocraft`，逐个应用补丁，冲突就地解决，再按上面的分区规则重新生成补丁（a 侧换成新版本）。

## 校验

重放结果必须与仓库内的树逐字节一致：

```sh
rm -rf /tmp/replay && mkdir -p /tmp/replay/upstream
git archive <tag> | tar -x -C /tmp/replay/upstream/photocraft
cd /tmp/replay && git init -q . && git add -A && git commit -qm base
for p in 0001-host-frame 0002-engine-hooks 0003-host-fonts 0004-chrome-sandbox-seams 0005-ohos-memory; do
  git apply /path/to/repo/patches/$p.patch || break
done
diff -rq --exclude=.git --exclude=target upstream/photocraft /path/to/repo/upstream/photocraft
```

`diff` 应无输出。若仓库工作树是 Windows 检出的 CRLF，差异只允许出现在换行符上；把补丁与基线统一成 LF 后应完全为空。

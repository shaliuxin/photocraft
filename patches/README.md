# 上游补丁系列

`upstream/photocraft/` 里的树是 **纯净上游 + 这里按序号应用的补丁**。补丁只放「上游必须知道的那点事」，鸿蒙自己的代码在 `native/rust/`（适配层）和 `entry/`（ArkTS 侧），不进补丁。

| 文件 | 内容 | 规模 |
|---|---|---|
| `0001-host-frame.patch` | 宿主扩展点：`eframe` 可选、`host_input`/`host_logic`/`host_ui`/`host_exit`、`file_dialog::ParentFrame`、`self-update` 特性、直接依赖 `egui-wgpu`/`wgpu`。已用 `cargo check -p photocraft-ui-egui --no-default-features` 对 aarch64 / x86_64-unknown-linux-ohos 两目标验过 | 171 增 / 85 删 / 15 文件 |
| `0002-engine-hooks.patch` | 引擎钩子：`installed_profiles::set_app_profile_dir`、`format::atomic::set_host_writer`、`text::cjk::set_ui_locale`、`i18n::set_host_locale`、ArkUI 主题枚举、手写笔偏好、`prefs/validate.rs` 的 OHOS 门控 | 356 增 / 24 删 / 7 文件 |
| `0003-host-fonts.patch` | 宿主字体：`text/cjk_fonts.rs` 的 `font_bytes_ok`、`store_host_cjk_font`、`reapply_host_fonts` | 77 增 / 1 文件 |
| `0004-chrome-sandbox-seams.patch` | 沙箱与外观：`Services` 的 `pick_open`/`pick_import`/`pick_save`/`pick_folder`/`sandbox_dir`/`documents_dir`/`path_pickers_only`/`always_pick_save`/`default_save`、`ask_file` 同步直答、`window_fullscreen`、`host_titlebar`、`native_doc_windows`、`ThemeKind::chrome_css`，以及 `path_pick` 的 166 行单测；宿主全屏经 View › Screen Mode（`F`）请求 `Services::window_fullscreen`（不发 eframe 的视口命令），宿主回报的状态再用 `adopt` 映回 Screen Mode | 778 增 / 2 删 / 14 文件 |

| `0005-brand-icon.patch` | 品牌图标反色：`assets/app-icon/hicolor/128x128/apps/ai.storyteller.photocraft.png` 的 RGB 反相（琥珀底 + 深蓝狐），alpha 保留；`brand.rs` 的 `include_bytes!` 与桌面 `icon_png` 共用此文件。二进制 `git diff --binary` 补丁，已在纯净树上验证逐像素一致 | 1 文件（二进制） |

合计 1,382 增 / 111 删，34 个文件（37 个改动点：`app.rs`/`file_dialog.rs`/`lib.rs` 由 0001 与 0004 各改一次）。v0.16 的分叉是 7,620 行 / 114 个文件。

四个文件都是纯 `git diff` 输出（`a/upstream/photocraft/...` 前缀，在仓库根用 `git apply -3` 应用），不含提交信息；每个补丁为什么存在，看这张表。

**手写笔没有补丁**：0.18 的 `app.stylus.feed` 就是平台后端的投递入口，鸿蒙侧只做线格式与屏幕轴倾角，见 `docs/native/host-stylus.md`。

## 为什么是补丁而不是分叉

v0.16 的做法是把上游整份拷下来改，改了 114 个文件、7620 行。上游 v0.16→v0.18 有 58 个文件冲突、72 个冲突块，`ui-egui` 还被重排成目录结构，14 个改过的文件直接变成 `modify/delete`——单 `lib.rs` 重放 54 个补丁块有 47 个失败。

同样的升级，补丁系列只需重放一个 150 行的补丁。

## 应用与重放

当前树已经应用过，正常构建不需要再跑。换上游版本时见 `docs/native/upstream-sync.md` 的「replaying the patch series」：把新版本解到 `upstream/photocraft`，`git apply -3` 逐个应用，冲突就地解决，然后用 `git format-patch` 覆盖对应补丁。

## 校验

重放结果必须与仓库内的树一致：

```sh
git archive <tag> | tar -x -C /tmp/new && cd /tmp/new && git init -q . && git add -A && git commit -qm base
git apply -3 /path/to/repo/patches/0001-host-frame.patch
diff -r --exclude=.git --exclude=target . /path/to/repo/upstream/photocraft
```

差异只允许出现在换行符上（Windows 检出的 CRLF）。

# 上游补丁系列

`upstream/photocraft/` 里的树是 **纯净上游 + 这里按序号应用的补丁**。补丁只放「上游必须知道的那点事」，鸿蒙自己的代码在 `native/rust/`（适配层）和 `entry/`（ArkTS 侧），不进补丁。

| 文件 | 内容 | 规模 |
|---|---|---|
| `0001-host-frame.patch` | 宿主扩展点：`eframe` 可选、`host_input`/`host_logic`/`host_ui`/`host_exit`、`file_dialog::ParentFrame`、`self-update` 特性、直接依赖 `egui-wgpu`/`wgpu` | 167 增 / 82 删 / 14 文件 |

后续各层（引擎钩子、字体语言、笔输入、沙箱文件、窗口外观）按 `docs/native/v0.18-port-plan.md` 的顺序加，一层一个补丁。

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

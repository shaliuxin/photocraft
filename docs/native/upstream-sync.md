# Upstream sync playbook

This fork carries upstream PhotoCraft as a Git subtree at `upstream/photocraft/`, plus a native
adapter in `native/rust/` that reuses upstream crates through path dependencies. Staying on the
newest upstream release is therefore a merge problem, not a port problem — but only if the two
places where versions leak are kept under control.

## What actually breaks on an upstream bump

Measured on `v0.16.0 → v0.18.0` (see the numbers at the end of this file):

1. **Dependency drift.** The adapter names `egui`, `egui-wgpu` and `wgpu` itself, because it paints
   with `egui-wgpu` instead of `eframe`. Upstream moves those versions. If the adapter keeps its old
   pins it compiles against a second copy of egui, and every type crossing the boundary stops
   matching. `v0.18.0` also introduced `[patch.crates-io]` entries that point egui and winit at
   reviewed forks; a separate workspace root does not inherit those, so the adapter has to copy them
   or it builds `photocraft-ui-egui` against crates.io egui while the desktop build uses the fork.
2. **A vendored crate named after a version.** `third_party/wgpu-hal-<version>/` carries the GLES
   uniform-block fix the HarmonyOS renderer needs. The directory name follows the wgpu version, so a
   wgpu bump needs a re-vendored copy before anything builds.
3. **File moves.** `v0.18.0` reorganised `crates/ui-egui` into module directories (`canvas/`,
   `dialogs/`, `menus/`, `panels/`, `input/`, `prefs/`, `tools/`, `text/`). A plain `git merge`
   reports `modify/delete` for every fork-edited file that moved, and Git cannot apply the fork's
   hunks to the new path.
4. **Version numbers in two files.** `AppScope/app.json5` (`versionName`/`versionCode`) and
   `native/rust/Cargo.toml` (`version`).

## The mechanical half: `scripts/upstream_sync.py`

```sh
python3 scripts/upstream_sync.py pins            # show drift
python3 scripts/upstream_sync.py pins --write    # rewrite egui/egui-wgpu/wgpu + [patch.crates-io]
python3 scripts/upstream_sync.py versions --write  # versionName/versionCode/app version from upstream
python3 scripts/upstream_sync.py check           # CI gate: fails on drift, missing vendored crate, lost abiFilters/deviceTypes
```

`pins` reads the versions upstream actually resolves from `upstream/photocraft/Cargo.lock`, not from
its manifest ranges, so the adapter tracks the same resolved graph the desktop build ships. Lines it
writes carry a `# pinned by scripts/upstream_sync.py` marker; the `[patch.crates-io]` table is
copied from upstream and the `wgpu-hal` line is rewritten to whatever vendored directory matches the
new wgpu version (and reported as missing when it does not exist yet).

Run `check` in CI after every subtree pull: it is the difference between "the adapter still builds
against upstream" and "the adapter silently builds against last month's egui".

## The judgement half: the merge itself

Follow `AGENTS.md`: syncs land on `upstream-track` through `git subtree pull --prefix=upstream/photocraft
upstream <tag>` **without `--squash`**, and are reviewed before they reach `main`.

Then, per conflicted file, pick one of three strategies:

| Situation | Strategy |
|---|---|
| Fork edit is a small `cfg!(feature = "harmonyos-ui")` guard | Re-apply by hand at the new location. |
| File moved upstream | Apply the fork's diff (`git diff <old-base> HEAD -- <old path>`) to the new path with `patch -p1 --fuzz=3`; check the rejects. |
| Fork edit is brand/i18n text only | Take upstream and re-add the fork's lines to the catalogs. |

Translation catalogs (`crates/ui-egui/src/i18n/*.tsv`) are appended by both sides, so they conflict
on nearly every release while carrying no logic. Prefer taking upstream's file and appending the
fork's HarmonyOS keys; keep the keys in one block per file so the next sync is a copy-paste.

## Keeping the next sync cheap

The fork currently edits 114 upstream files (7,620 changed lines) — that is the whole cost of a
sync. Almost all of it falls into three groups, and only the third needs to live in upstream files:

* **Brand and i18n text** (`brand.rs`, `links.rs`, `i18n/*.tsv`): could be produced by the adapter
  at build time from an overlay instead of being patched in.
* **Visibility tweaks** (hide promotion links and credit tabs, theme entries): several are already
  behind `harmonyos-ui`; each one is a future conflict.
* **Host integration** (`app.host_logic`, `app.host_input`, `app.host_ui`, `app.host_titlebar`,
  `app.services.*`, the optional `eframe-host` feature): this is the part that genuinely must be
  upstream, and it is small — five entry points plus one Cargo feature.

Keeping that seam stable and pushing everything else into the adapter is what makes requirement
"future upstream releases adapt with little work" true. Every `cfg!(harmonyos-ui)` added to an
upstream file is a hunk the next sync has to carry by hand.

## ABI and device checklist

Both live in one place each; a new ABI or device type touches all of them:

* `entry/src/main/cpp/CMakeLists.txt` — `OHOS_ARCH` → Rust triple.
* `scripts/build_native.sh` — accepted triples (and the default list).
* `entry/build-profile.json5` — `abiFilters`.
* `scripts/verify_native.py` — `ELF_MACHINE`, `DEFAULT_ABIS`.
* `entry/src/main/module.json5` — `deviceTypes`.

## Measured sync cost, v0.16.0 → v0.18.0

| Item | Value |
|---|---|
| Upstream files changed | 1,262 |
| Files this fork edits under `upstream/photocraft` | 114 (7,620 lines) |
| Files both sides touch | 63 |
| Merge conflicts (`git merge` of `v0.18.0` into the fork) | 58 files, 72 hunks |
| Fork-edited files upstream moved or deleted | 14 (`modify/delete`) |
| egui / wgpu | 0.36.2 → 0.37.0 / 30.0.1 (unchanged) |
| New `[patch.crates-io]` entries upstream | egui family + winit forks |

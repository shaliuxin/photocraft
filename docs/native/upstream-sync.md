# Upstream sync playbook

The repository carries upstream PhotoCraft at `upstream/photocraft/`, plus a native adapter in
`native/rust/` that reuses upstream crates through path dependencies. Since v0.18.0 the carried
tree is **pristine upstream with a small patch series applied** (`patches/*.patch`), not a fork:
the port keeps its own code in the adapter and asks upstream for exactly one thing, a host seam.

That changes an upgrade from a merge into a replay. `v0.16.0`'s fork edited 114 upstream files
(7,620 lines) and cost 72 conflict hunks per release; the v0.18.0 host patch is 167 insertions and
82 deletions in 14 files and applies in one shot. See `docs/native/v0.18-port-plan.md` for the
measurement and for what is still unported.

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

## The judgement half: replaying the patch series

`patches/` is the recipe. To move to a new upstream tag:

```sh
# 1. a clean tree of the release, and a scratch repo to replay in
git archive <new-tag> | tar -x -C /tmp/upstream-new

# 2. replace the carried tree and replay in order; -3 leaves conflict markers
rm -rf upstream/photocraft && cp -a /tmp/upstream-new upstream/photocraft
for p in patches/*.patch; do
  git apply -3 --directory=upstream/photocraft "$p" || echo "CONFLICT in $p"
done

# 3. the mechanical half, then the gate
python3 scripts/upstream_sync.py pins --write
python3 scripts/upstream_sync.py versions --write
python3 scripts/upstream_sync.py check
```

A conflict here is a real answer — the upstream line the patch touches has changed — and it is
scoped to one hunk instead of one of seventy-two. Resolve it in place, then regenerate the patch:

```sh
git diff -- upstream/photocraft/crates/ui-egui > patches/0001-host-frame.patch   # or `git format-patch`
```

Rules for keeping the series replayable:

| Situation | Strategy |
|---|---|
| Patch adds a host seam (a method, a feature, an extension point) | Keep it in `patches/`; it is the only thing upstream must carry. |
| Patch adds a `cfg!(harmonyos-ui)` guard to an upstream file | Move the behaviour into the adapter instead; a guard is a hunk the next release has to carry. |
| Patch is brand/i18n text | Ship it from the adapter at build time (an overlay), never as a patch hunk. |

Translation catalogs (`crates/ui-egui/src/i18n/*.tsv`) are the worst case: both sides append, so
they conflict on nearly every release while carrying no logic. Overlaying them from the adapter
removes that entire class.

### Two traps hit on v0.20.1 → v0.21.0

* **CRLF breaks `git apply --3way`.** A Windows checkout (`core.autocrlf=true`) hands `git apply`
  files with CRLF while the patches are LF. Every hunk then reports `patch failed` /
  `Falling back to direct application...`, which looks like a huge conflict but is only line
  endings. Normalise the baseline, ours and theirs to LF before replaying. The reliable route is a
  per-file three-way merge with `git merge-file -p <ours> <base> <theirs>` (`base` = pristine old
  upstream, `ours` = pristine new upstream, `theirs` = the validated old host tree); it produced
  4 files / 7 real conflicts, not the dozens `git apply` appeared to report.
* **`--locked` in `scripts/build_native.sh` fails after the upstream crate version changes.**
  `native/rust/Cargo.lock` records the path-dependency versions of every `photocraft-*` crate, so
  bumping the workspace version invalidates it. Run `cargo metadata --format-version 1` once in
  `native/rust/` (no `--locked`) to rewrite the lockfile before calling `dev.sh build`, otherwise
  the build aborts with `cannot update the lock file ... because --locked was passed`.

Also re-check any **new upstream module** that reaches for `eframe::wgpu` directly: the host build
has `eframe` optional and links `wgpu` itself, so such a file must use `wgpu::*` (v0.21.0's new
`canvas/gpu_canvas/compositor_build.rs` needed exactly this).

## Port layers

The port's own code is layered so each layer is one patch and one verification:

| Layer | Where | State |
|---|---|---|
| Host seam | `patches/0001-host-frame.patch` | done, builds on both ABIs |
| Engine/format hooks | `patches/` (to add) | 144 lines |
| Fonts and locale | `patches/` (to add) | 286 lines |
| Pen input | `patches/` (to add) | 1,106 lines (can be ~200 if upstream's stylus is reused) |
| Sandbox file access | `patches/` (to add) | 914 lines |
| Window chrome and themes | `patches/` (to add) | 1,071 lines |

## Why the fork is gone

The v0.16 fork edited 114 upstream files (7,620 changed lines) — that was the whole cost of a
sync. Almost all of it fell into three groups, and only the third needs to live in upstream files:

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

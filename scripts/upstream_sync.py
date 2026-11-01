#!/usr/bin/env python3
"""Keep the HarmonyOS adapter in step with upstream PhotoCraft.

The adapter is a separate Cargo workspace that reuses upstream crates through path dependencies.
Two things drift every time upstream moves, and both used to be edited by hand:

* the egui / egui-wgpu / wgpu / craft-libs versions, plus the `[patch.crates-io]` block that keeps
  the adapter on the same forked egui as the desktop build;
* the `AppScope/app.json5` and adapter version numbers.

This script derives both from upstream instead of trusting memory, then reports what a subtree
merge will need. It never resolves semantic conflicts: it makes them visible and countable.

Usage:
    scripts/upstream_sync.py pins            # show drift between upstream and the adapter
    scripts/upstream_sync.py pins --write    # rewrite the generated block in native/rust/Cargo.toml
    scripts/upstream_sync.py versions --write  # set app/adapter versions from upstream
    scripts/upstream_sync.py check           # pins + versions + vendored wgpu-hal, exit 1 on drift
    scripts/upstream_sync.py plan --tag v0.18.0  # what to expect from a subtree merge
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
UPSTREAM = ROOT / "upstream" / "photocraft"
ADAPTER_MANIFEST = ROOT / "native" / "rust" / "Cargo.toml"
APP_JSON = ROOT / "AppScope" / "app.json5"
ADAPTER_MANIFEST_VERSION = re.compile(r'^version = "([^"]+)"', re.MULTILINE)

# Each tracked dependency line carries this marker, and the [patch.crates-io] table is compared as a
# whole: both are rewritten by `pins --write`. A marker beats a block here because the version lines
# must live inside [dependencies] while the patch table is a top-level table of its own.
MARK = " # pinned by scripts/upstream_sync.py"

# Crates the adapter names directly instead of pulling in through `photocraft-ui-egui`. Each one
# must track the version upstream resolves, or the adapter compiles against a second copy of egui.
TRACKED = ("egui", "egui-wgpu", "wgpu")

# Features the adapter itself needs, which upstream's version number says nothing about. `gles` is
# what lets wgpu reach the EGL surface XComponent hands over; without it the renderer has no backend
# on HarmonyOS at all.
REQUIRED_FEATURES = {
    "egui": ["default_fonts"],
    "wgpu": ["std", "wgsl", "gles"],
}


def read(path: Path) -> str:
    """File text with LF endings: a CRLF checkout would break the line-anchored regexes below."""
    return path.read_text(encoding="utf-8").replace("\r\n", "\n")


def write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8", newline="\n")


def cargo_section(text: str, header: str) -> str:
    """Body of a TOML table, from its header line to the next table header or EOF.

    Line-based on purpose: `\\s*$` in a multiline regex happily eats the newline after the header
    and then matches a table boundary in the wrong place.
    """
    lines = text.split("\n")
    for index, line in enumerate(lines):
        if line.strip() == f"[{header}]":
            body = []
            for following in lines[index + 1:]:
                if following.startswith("["):
                    break
                body.append(following)
            return "\n".join(body)
    return ""


def workspace_dep(text: str, name: str) -> str | None:
    body = cargo_section(text, "workspace.dependencies")
    match = re.search(rf'^{re.escape(name)}\s*=\s*(.+)$', body, re.MULTILINE)
    return match.group(1).strip() if match else None


def lock_versions(text: str) -> dict[str, str]:
    """name -> version from a Cargo.lock, first occurrence wins."""
    versions: dict[str, str] = {}
    for block in text.split("[[package]]"):
        name = re.search(r'^name = "([^"]+)"', block, re.MULTILINE)
        version = re.search(r'^version = "([^"]+)"', block, re.MULTILINE)
        if name and version:
            versions.setdefault(name.group(1), version.group(1))
    return versions


def upstream_pins() -> dict[str, str]:
    """Versions upstream actually resolves, read from its lockfile."""
    lock = UPSTREAM / "Cargo.lock"
    if not lock.exists():
        raise SystemExit(f"missing {lock}; the subtree is not checked out")
    versions = lock_versions(read(lock))
    missing = [name for name in TRACKED if name not in versions]
    if missing:
        raise SystemExit(f"upstream lockfile has no version for: {', '.join(missing)}")
    return {name: versions[name] for name in TRACKED}


def upstream_patch_block() -> str:
    """Upstream's `[patch.crates-io]` table, verbatim.

    The desktop build patches egui (and winit) to reviewed forks. The adapter has its own workspace
    root, so upstream's patch section does not apply to it: without a copy here the adapter would
    compile `photocraft-ui-egui` against crates.io egui while the desktop build uses the fork, and
    any fork-only API upstream starts using breaks the HarmonyOS build.
    """
    return cargo_section(read(UPSTREAM / "Cargo.toml"), "patch.crates-io").strip()


def app_version() -> str:
    return json.loads(read(APP_JSON))["app"]["versionName"]


def dep_line(name: str, version: str) -> str:
    """The [dependencies] line the adapter should carry for a tracked crate.

    Only the version comes from upstream. The adapter's own feature requirements are merged back in:
    a version drift and a feature drift fail differently, and the feature one only shows up when the
    renderer cannot reach EGL at runtime.
    """
    parts = [f'version = "{version}"']
    if name != "egui":
        parts.append("default-features = false")
    features = REQUIRED_FEATURES.get(name)
    if features:
        rendered = ", ".join(f'"{feature}"' for feature in features)
        parts.append(f"features = [{rendered}]")
    return f"{name} = {{ {', '.join(parts)} }}{MARK}"


def patch_section(pins: dict[str, str]) -> str:
    """The [patch.crates-io] table the adapter should carry.

    Upstream already patches wgpu-hal to the vendored crate when this fork needs the GLES
    uniform-block fix, so that line is replaced rather than appended: the vendored directory is
    named after the wgpu version and has to follow it.
    """
    lines = ["[patch.crates-io]"]
    for line in upstream_patch_block().splitlines():
        if line.strip().startswith("wgpu-hal"):
            continue
        if line.strip():
            lines.append(line)
    hal = vendored_wgpu_hal(pins["wgpu"])
    if hal is not None:
        lines.append(f'wgpu-hal = {{ path = "{hal.as_posix()}" }}')
    return "\n".join(lines) + "\n"


def vendored_wgpu_hal(wgpu_version: str) -> str | None:
    """Relative path of the vendored wgpu-hal for this wgpu version, if it is present.

    The OHOS GLES path needs the uniform-block fix carried by `third_party/wgpu-hal-<version>`.
    A wgpu bump therefore needs a matching vendored crate before the adapter can build.
    """
    candidate = ROOT / "third_party" / f"wgpu-hal-{wgpu_version}"
    if not candidate.is_dir():
        return None
    return Path("../../third_party") / candidate.name


def manifest_drift(text: str, pins: dict[str, str]) -> list[str]:
    """What differs between the adapter manifest and what upstream implies."""
    drift = []
    for name in TRACKED:
        match = re.search(rf"^{re.escape(name)}\s*=\s*\{{.*$", text, re.MULTILINE)
        if match is None or match.group(0) != dep_line(name, pins[name]):
            drift.append(name)
    have = cargo_section(text, "patch.crates-io").strip()
    want = cargo_section(patch_section(pins), "patch.crates-io").strip()
    if have != want:
        drift.append("[patch.crates-io]")
    return drift


def rewrite_manifest(text: str, pins: dict[str, str]) -> str:
    for name in TRACKED:
        text = re.sub(rf"^{re.escape(name)}\s*=\s*\{{.*$", dep_line(name, pins[name]), text,
                      count=1, flags=re.MULTILINE)
    lines = text.split("\n")
    replacement = patch_section(pins).rstrip("\n").split("\n")
    start = next((i for i, line in enumerate(lines) if line.strip() == "[patch.crates-io]"), None)
    if start is None:
        return text.rstrip("\n") + "\n\n" + patch_section(pins)
    end = start + 1
    while end < len(lines) and not lines[end].startswith("["):
        end += 1
    tail = [""] + lines[end:] if end < len(lines) and lines[end].startswith("[") else lines[end:]
    return "\n".join(lines[:start] + replacement + tail)


def cmd_pins(args: argparse.Namespace) -> int:
    pins = upstream_pins()
    text = read(ADAPTER_MANIFEST)
    drift = manifest_drift(text, pins)
    if not drift:
        print("adapter pins match upstream " + ", ".join(f"{k} {v}" for k, v in pins.items()))
        return 0
    if args.write:
        write(ADAPTER_MANIFEST, rewrite_manifest(text, pins))
        print("rewrote native/rust/Cargo.toml pins: " + ", ".join(drift))
        return 0
    print("adapter is out of step with upstream in: " + ", ".join(drift))
    for name in TRACKED:
        print(f"  {dep_line(name, pins[name])}")
    print("\nrun `scripts/upstream_sync.py pins --write` to update them")
    return 1


def cmd_versions(args: argparse.Namespace) -> int:
    """versionName is <upstream major.minor.patch>.<hmos revision>; versionCode packs the same."""
    upstream_version = json.loads(re.search(
        r"^version = \"([^\"]+)\"", read(UPSTREAM / "Cargo.toml"), re.MULTILINE).group(0).split('"')[1])
    current = app_version()
    upstream_parts = [int(p) for p in upstream_version.split(".")[:3]]
    current_parts = [int(p) for p in current.split(".")]
    revision = current_parts[3] if len(current_parts) > 3 and current_parts[:3] == upstream_parts else 1
    target = ".".join(str(p) for p in upstream_parts + [revision])
    code = upstream_parts[0] * 1_000_000 + upstream_parts[1] * 10_000 + upstream_parts[2] * 100 + revision
    if target == current:
        print(f"versions already at {target}")
        return 0
    print(f"versionName {current} -> {target}, versionCode -> {code}")
    if not args.write:
        print("run with --write to apply")
        return 1
    app = json.loads(read(APP_JSON))
    app["app"]["versionName"] = target
    app["app"]["versionCode"] = code
    write(APP_JSON, json.dumps(app, indent=2) + "\n")
    manifest = read(ADAPTER_MANIFEST)
    manifest = ADAPTER_MANIFEST_VERSION.sub(f'version = "{upstream_version}"', manifest, count=1)
    write(ADAPTER_MANIFEST, manifest)
    print("updated AppScope/app.json5 and native/rust/Cargo.toml")
    return 0


def cmd_check(args: argparse.Namespace) -> int:
    failures = []
    pins = upstream_pins()
    if manifest_drift(read(ADAPTER_MANIFEST), pins):
        failures.append("native/rust/Cargo.toml pins do not match upstream (run: pins --write)")
    if vendored_wgpu_hal(pins["wgpu"]) is None:
        failures.append(
            f"third_party/wgpu-hal-{pins['wgpu']} is missing: re-vendor the uniform-block fix for "
            "this wgpu version before building")
    upstream_version = re.search(r'^version = "([^"]+)"', read(UPSTREAM / "Cargo.toml"), re.MULTILINE).group(1)
    if not app_version().startswith(upstream_version + "."):
        failures.append(
            f"AppScope versionName {app_version()} does not derive from upstream {upstream_version} "
            "(run: versions --write)")
    for method in ("abiFilters", "deviceTypes"):
        owners = ("entry/build-profile.json5",) if method == "abiFilters" else ("entry/src/main/module.json5",)
        for owner in owners:
            if method not in read(ROOT / owner):
                failures.append(f"{owner} lost its {method} entry")
    for failure in failures:
        print(f"FAIL {failure}")
    if not failures:
        print("adapter configuration matches upstream")
    return 1 if failures else 0


def cmd_plan(args: argparse.Namespace) -> int:
    """Report the files upstream touched that this fork also touches.

    A subtree merge conflicts where both sides edited the same file, and silently drops fork edits
    where upstream deleted or moved a file. Both lists are printed so the merge can be scheduled
    instead of discovered.
    """
    if args.tag:
        ref = f"refs/tags/{args.tag}"
    else:
        ref = "upstream/main"
    changed = subprocess.run(
        ["git", "-C", str(ROOT), "diff", "--name-only", f"{args.base}..{ref}", "--", "upstream/photocraft"],
        capture_output=True, text=True)
    if changed.returncode != 0:
        print(changed.stderr.strip(), file=sys.stderr)
        print("fetch the tag first: git fetch --depth 1 ../upstream refs/tags/<tag>", file=sys.stderr)
        return 2
    files = [line.split("upstream/photocraft/", 1)[-1] for line in changed.stdout.split() if line]
    print(f"upstream changed {len(files)} files under upstream/photocraft between {args.base} and {ref}")
    print("expect conflicts in the files this fork also edits; see docs/native/upstream-sync.md")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    pins = sub.add_parser("pins", help="compare or rewrite the adapter's dependency pins")
    pins.add_argument("--write", action="store_true")
    pins.set_defaults(func=cmd_pins)

    versions = sub.add_parser("versions", help="derive app/adapter versions from upstream")
    versions.add_argument("--write", action="store_true")
    versions.set_defaults(func=cmd_versions)

    check = sub.add_parser("check", help="fail when the adapter drifted from upstream")
    check.set_defaults(func=cmd_check)

    plan = sub.add_parser("plan", help="list files upstream changed since a base ref")
    plan.add_argument("--base", default="HEAD")
    plan.add_argument("--tag")
    plan.set_defaults(func=cmd_plan)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Unit checks for scripts/upstream_sync.py.

Run with: python3 scripts/test_upstream_sync.py
"""
import importlib.util
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "upstream_sync", Path(__file__).resolve().parent / "upstream_sync.py")
sync = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sync)

MANIFEST = """[dependencies]
egui = { version = "0.36", features = ["default_fonts"] }
egui-wgpu = { version = "0.36", default-features = false }
wgpu = { version = "30", default-features = false, features = ["std", "wgsl", "gles"] }
pollster = "1"

[patch.crates-io]
wgpu-hal = { path = "../../third_party/wgpu-hal-30.0.1" }

[profile.release]
lto = "thin"
"""

LOCK = """[[package]]
name = "egui"
version = "0.37.0"

[[package]]
name = "egui-wgpu"
version = "0.37.0"

[[package]]
name = "wgpu"
version = "30.0.1"
"""


class ManifestTests(unittest.TestCase):
    def test_drift_lists_the_tracked_crates_and_the_patch_table(self):
        pins = {"egui": "0.37.0", "egui-wgpu": "0.37.0", "wgpu": "30.0.1"}
        drift = sync.manifest_drift(MANIFEST, pins)
        self.assertIn("egui", drift)
        self.assertIn("egui-wgpu", drift)
        # wgpu keeps its version, so only the dependency line formatting decides this one.
        self.assertIn("wgpu", drift)

    def test_rewrite_is_idempotent_and_keeps_the_rest_of_the_manifest(self):
        pins = {"egui": "0.37.0", "egui-wgpu": "0.37.0", "wgpu": "30.0.1"}
        once = sync.rewrite_manifest(MANIFEST, pins)
        twice = sync.rewrite_manifest(once, pins)
        self.assertEqual(once, twice, "a second rewrite must not change anything")
        self.assertIn('egui = { version = "0.37.0"', once)
        self.assertIn('pollster = "1"', once)
        self.assertIn("[profile.release]", once)
        self.assertEqual(once.count("[patch.crates-io]"), 1, "the patch table must not duplicate")
        hal_lines = [line for line in once.split("\n") if line.startswith("wgpu-hal =")]
        self.assertEqual(len(hal_lines), 1, "the vendored patch must not repeat")

    def test_upstream_patch_entries_are_carried_over(self):
        # Upstream v0.18 patches the egui family to forks; the adapter needs the same pins.
        pins = {"egui": "0.37.0", "egui-wgpu": "0.37.0", "wgpu": "30.0.1"}
        original = sync.upstream_patch_block
        sync.upstream_patch_block = lambda: 'egui = { git = "https://example.invalid/egui", rev = "abc" }'
        try:
            text = sync.rewrite_manifest(MANIFEST, pins)
        finally:
            sync.upstream_patch_block = original
        self.assertIn("example.invalid/egui", text)


class ParsingTests(unittest.TestCase):
    def test_cargo_section_stops_at_the_next_table(self):
        body = sync.cargo_section(MANIFEST, "patch.crates-io")
        self.assertIn("wgpu-hal", body)
        self.assertNotIn("[profile.release]", body)

    def test_missing_section_is_empty_not_an_error(self):
        self.assertEqual(sync.cargo_section(MANIFEST, "workspace.dependencies"), "")

    def test_lock_versions_reads_the_resolved_graph(self):
        versions = sync.lock_versions(LOCK)
        self.assertEqual(versions["egui"], "0.37.0")
        self.assertEqual(versions["wgpu"], "30.0.1")


if __name__ == "__main__":
    unittest.main(verbosity=2)

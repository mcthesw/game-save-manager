"""Exercise the publication selector against uploaded artifact subdirectories."""

import tempfile
import unittest
from pathlib import Path

from build_stable_manifest import nightly_assets
from test_build_stable_manifest import FILENAMES


class NightlyAssetsTests(unittest.TestCase):
    def test_nested_installers_are_included_without_signatures(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for kind, name in FILENAMES.items():
                path = root / kind / name
                path.parent.mkdir()
                path.write_bytes(b"installer")
                path.with_name(name + ".sig").write_text("signature")
            (root / "RGSM_1.9.1_aarch64.app.tar.gz").write_bytes(b"archive")
            (root / "LICENSE").write_text("license")
            selected = nightly_assets(root)
            self.assertEqual(len(selected), 9)
            self.assertTrue(set(FILENAMES.values()).issubset({p.name for p in selected}))

    def test_missing_installer_blocks_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                nightly_assets(Path(directory))

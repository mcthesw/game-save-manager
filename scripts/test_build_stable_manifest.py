import tempfile
import unittest
from pathlib import Path

from build_stable_manifest import assemble


FILENAMES = {
    "nsis": "RGSM_1.9.1_x64-setup.exe",
    "msi": "RGSM_1.9.1_x64_en-US.msi",
    "appimage": "RGSM_1.9.1_amd64.AppImage",
    "deb": "RGSM_1.9.1_amd64.deb",
    "rpm": "RGSM-1.9.1-1.x86_64.rpm",
    "dmg": "RGSM_1.9.1_aarch64.dmg",
    "portable": "RGSM_1.9.1_x64-portable-slim.zip",
}


class StableManifestTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.assets = Path(self.temp.name)
        for name in FILENAMES.values():
            (self.assets / name).write_bytes(b"bundle")
        for kind in ("nsis", "msi", "appimage"):
            (self.assets / f"{FILENAMES[kind]}.sig").write_text("signature")

    def test_manifest_keeps_each_installer_format(self):
        manifest = assemble(self.assets, "v1.9.1", "owner/repo")
        self.assertTrue(manifest["platforms"]["windows-x86_64-msi"]["url"].endswith(".msi"))
        self.assertTrue(manifest["platforms"]["windows-x86_64-nsis"]["url"].endswith(".exe"))
        self.assertTrue(manifest["platforms"]["linux-x86_64-appimage"]["url"].endswith(".AppImage"))
        self.assertTrue(manifest["downloads"]["darwin-aarch64-dmg"].endswith(".dmg"))

    def test_missing_signature_blocks_publication(self):
        (self.assets / f"{FILENAMES['msi']}.sig").unlink()
        with self.assertRaisesRegex(ValueError, "Missing updater signature"):
            assemble(self.assets, "v1.9.1", "owner/repo")

    def test_wrong_version_blocks_publication(self):
        with self.assertRaisesRegex(ValueError, "does not contain"):
            assemble(self.assets, "v1.9.2", "owner/repo")


if __name__ == "__main__":
    unittest.main()

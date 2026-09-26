"""Assemble one verified static updater manifest from complete release artifacts."""

import argparse
import json
import sys
from pathlib import Path
from urllib.parse import quote


ASSETS = {
    "nsis": "RGSM_*_x64-setup.exe",
    "msi": "RGSM_*_x64_en-US.msi",
    "appimage": "RGSM_*.AppImage",
    "deb": "RGSM_*.deb",
    "rpm": "RGSM-*.rpm",
    "dmg": "RGSM_*.dmg",
    "portable": "RGSM_*_x64-portable-slim.zip",
}
SIGNED = {"nsis", "msi", "appimage"}
PLATFORMS = {
    "nsis": "windows-x86_64-nsis",
    "msi": "windows-x86_64-msi",
    "appimage": "linux-x86_64-appimage",
}
DOWNLOADS = {
    **PLATFORMS,
    "deb": "linux-x86_64-deb",
    "rpm": "linux-x86_64-rpm",
    "dmg": "darwin-aarch64-dmg",
    "portable": "windows-x86_64-portable-slim",
}


def nightly_assets(assets: Path) -> list[Path]:
    selected = []
    for pattern in [*ASSETS.values(), "*.app.tar.gz", "LICENSE"]:
        matches = [path for path in assets.rglob(pattern) if path.is_file()]
        if len(matches) != 1:
            raise ValueError(f"Expected one {pattern}, found {len(matches)}")
        selected.extend(matches)
    return selected


def assemble(assets: Path, tag: str, repository: str) -> dict:
    version = tag.removeprefix("v")
    found: dict[str, Path] = {}
    for kind, pattern in ASSETS.items():
        matches = list(assets.rglob(pattern))
        if len(matches) != 1:
            raise ValueError(f"Expected one {kind} artifact, found {len(matches)}")
        artifact = matches[0]
        version_marker = f"-{version}-" if kind == "rpm" else f"_{version}_"
        if version_marker not in artifact.name:
            raise ValueError(f"{artifact.name} does not contain {version}")
        found[kind] = artifact

    names = [path.name for path in found.values()]
    if len(names) != len(set(names)):
        raise ValueError("Duplicate release asset names")

    def url(path: Path) -> str:
        return f"https://github.com/{repository}/releases/download/{tag}/{quote(path.name)}"

    platforms = {}
    for kind in SIGNED:
        artifact = found[kind]
        signature = artifact.with_name(artifact.name + ".sig")
        if not signature.is_file():
            raise ValueError(f"Missing updater signature for {artifact.name}")
        value = signature.read_text(encoding="utf-8").strip()
        if not value:
            raise ValueError(f"Empty updater signature for {artifact.name}")
        platforms[PLATFORMS[kind]] = {"url": url(artifact), "signature": value}

    return {
        "version": version,
        "platforms": platforms,
        "downloads": {target: url(found[kind]) for kind, target in DOWNLOADS.items()},
        "releasePage": f"https://github.com/{repository}/releases/tag/{tag}",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--nightly-files", action="store_true")
    parser.add_argument("--assets", required=True, type=Path)
    parser.add_argument("--tag")
    parser.add_argument("--repository")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.nightly_files:
        for path in nightly_assets(args.assets):
            sys.stdout.buffer.write(str(path).encode("utf-8") + b"\0")
        return
    if not all([args.tag, args.repository, args.output]):
        parser.error("--tag, --repository and --output are required for a manifest")
    manifest = assemble(args.assets, args.tag, args.repository)
    args.output.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Verified {len(ASSETS)} installers and {len(SIGNED)} updater signatures")


if __name__ == "__main__":
    main()

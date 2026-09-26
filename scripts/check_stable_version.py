"""Reject an accidental or non-incrementing stable publication."""

import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path


def numeric_version(value: str) -> tuple[int, int, int]:
    if not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", value):
        raise ValueError(f"Stable version must be X.Y.Z: {value}")
    return tuple(map(int, value.split(".")))


def main() -> None:
    version = sys.argv[1]
    numeric_version(version)
    with Path("Cargo.toml").open("rb") as source:
        package_version = tomllib.load(source)["workspace"]["package"]["version"]
    if version != package_version:
        raise ValueError(f"Input {version} does not match Cargo.toml {package_version}")

    repository = os.environ["GITHUB_REPOSITORY"]
    tag = f"v{version}"
    existing = subprocess.run(
        ["gh", "release", "view", tag, "--repo", repository], capture_output=True, check=False
    )
    if existing.returncode == 0:
        raise ValueError(f"Release {tag} already exists")
    existing_tag = subprocess.run(
        ["gh", "api", f"repos/{repository}/git/ref/tags/{tag}"],
        capture_output=True,
        check=False,
    )
    if existing_tag.returncode == 0:
        raise ValueError(f"Tag {tag} already exists")

    latest = subprocess.run(
        ["gh", "api", f"repos/{repository}/releases/latest"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    if latest.returncode != 0:
        raise RuntimeError("Could not read the latest stable release")
    latest_version = json.loads(latest.stdout)["tag_name"].removeprefix("v")
    if numeric_version(version) <= numeric_version(latest_version):
        raise ValueError(f"{tag} must be newer than latest stable v{latest_version}")
    print(f"Ready to publish {tag}")


if __name__ == "__main__":
    main()

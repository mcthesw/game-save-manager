#!/usr/bin/env python3
"""Generate nightly release notes relative to the latest stable release."""

from __future__ import annotations

import argparse
import subprocess
from pathlib import Path


def git(*args: str, cwd: Path | None = None) -> str:
    result = subprocess.run(
        ["git", *args],
        check=False,
        cwd=cwd,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"git {' '.join(args)} failed with exit code {result.returncode}:\n"
            f"{result.stderr.strip()}"
        )
    return result.stdout.strip()


def resolve_commit(ref: str, cwd: Path | None = None) -> str:
    return git("rev-parse", "--verify", f"refs/tags/{ref}^{{commit}}", cwd=cwd)


def changelog_since_stable(stable_ref: str, cwd: Path | None = None) -> str:
    stable_commit = resolve_commit(stable_ref, cwd=cwd)

    # Use a symmetric-difference range with patch-equivalence filtering. This
    # keeps nightly notes correct even when the default branch was rebased or
    # rebuilt and the stable tag is no longer an ancestor of HEAD.
    return git(
        "log",
        "--cherry-pick",
        "--right-only",
        f"{stable_commit}...HEAD",
        "--pretty=format:- %s (%h)",
        "--no-merges",
        cwd=cwd,
    )


def release_notes(stable_ref: str, cwd: Path | None = None) -> str:
    changelog = changelog_since_stable(stable_ref, cwd=cwd)
    if not changelog:
        changelog = "No changes since last stable release."

    return f"## Changes since last stable release ({stable_ref})\n\n{changelog}\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--stable-tag", required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    notes = release_notes(args.stable_tag)
    if args.output:
        args.output.write_text(notes, encoding="utf-8")
    else:
        print(notes, end="")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

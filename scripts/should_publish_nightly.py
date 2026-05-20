#!/usr/bin/env python3
"""Decide whether the nightly release workflow should publish a build."""

from __future__ import annotations

import argparse
import os
import subprocess
from pathlib import Path

RELEASE_RELEVANT_PATHS = [
    "apps/rgsm-gui",
    "crates",
    "Cargo.toml",
    "Cargo.lock",
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "scripts",
    "locales",
    ".github/workflows/build-gui.yml",
    ".github/workflows/publish-nightly.yml",
]


def git(*args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *args],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if check and result.returncode != 0:
        raise RuntimeError(
            f"git {' '.join(args)} failed with exit code {result.returncode}:\n"
            f"{result.stderr.strip()}"
        )
    return result.stdout.strip()


def append_github_output(values: dict[str, str]) -> None:
    output_path = os.environ.get("GITHUB_OUTPUT")
    lines = [f"{key}={value}" for key, value in values.items()]

    if output_path:
        with Path(output_path).open("a", encoding="utf-8") as output_file:
            for line in lines:
                print(line, file=output_file)
    else:
        for line in lines:
            print(line)


def append_step_summary(markdown: str) -> None:
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary_path:
        with Path(summary_path).open("a", encoding="utf-8") as summary_file:
            print(markdown, file=summary_file)
    else:
        print(markdown)


def publish(reason: str) -> None:
    append_github_output({"should_publish": "true", "reason": reason})


def skip(reason: str) -> None:
    append_github_output({"should_publish": "false", "reason": reason})


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--event-name", default=os.environ.get("GITHUB_EVENT_NAME", ""))
    parser.add_argument("--release-tag", default="latest-built")
    args = parser.parse_args()

    if args.event_name == "workflow_dispatch":
        print("Manual dispatch requested; publishing nightly regardless of changes.")
        publish("manual dispatch")
        return 0

    git("fetch", "--tags", "--force", "--quiet")

    base_sha = git(
        "rev-parse",
        "-q",
        "--verify",
        f"refs/tags/{args.release_tag}^{{commit}}",
        check=False,
    )
    if not base_sha:
        print(f"No {args.release_tag} baseline tag found; publishing nightly.")
        publish(f"no {args.release_tag} baseline tag")
        return 0

    head_sha = os.environ.get("GITHUB_SHA") or git("rev-parse", "HEAD")
    changed_files = git(
        "diff",
        "--name-only",
        f"{base_sha}..HEAD",
        "--",
        *RELEASE_RELEVANT_PATHS,
    ).splitlines()

    summary = [
        "### Nightly change detection",
        "",
        f"Base `{args.release_tag}` successful nightly tag: `{base_sha}`",
        f"Head: `{head_sha}`",
        "",
    ]

    if not changed_files:
        print("No release-relevant changes since latest successful nightly; skipping publish.")
        summary.append("No release-relevant changes since latest successful nightly.")
        append_step_summary("\n".join(summary))
        skip("no release-relevant changes")
        return 0

    print("Release-relevant changes found; publishing nightly.")
    summary.extend(
        [
            "Release-relevant changed files:",
            "```",
            *changed_files,
            "```",
        ]
    )
    append_step_summary("\n".join(summary))
    publish("release-relevant changes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

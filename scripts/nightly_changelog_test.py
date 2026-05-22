#!/usr/bin/env python3
"""Regression tests for nightly changelog generation."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from nightly_changelog import changelog_since_stable, git


def run_git(repo: Path, *args: str) -> str:
    return git(*args, cwd=repo)


def write_file(repo: Path, relative_path: str, content: str) -> None:
    path = repo / relative_path
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def commit_file(repo: Path, relative_path: str, content: str, message: str) -> None:
    write_file(repo, relative_path, content)
    run_git(repo, "add", relative_path)
    run_git(repo, "commit", "-m", message)


class NightlyChangelogTests(unittest.TestCase):
    def test_excludes_stable_changes_after_history_rewrite(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            run_git(repo, "init")
            run_git(repo, "config", "user.email", "ci@example.com")
            run_git(repo, "config", "user.name", "CI")

            commit_file(repo, "README.md", "root\n", "chore: initial")
            root = run_git(repo, "rev-parse", "HEAD")

            run_git(repo, "switch", "-c", "stable")
            commit_file(repo, "release.txt", "stable branch\n", "chore: release branch marker")
            commit_file(repo, "app.txt", "stable feature\n", "feat: stable baseline")
            run_git(repo, "tag", "v1.0.0")

            run_git(repo, "switch", "-c", "dev", root)
            commit_file(repo, "app.txt", "stable feature\n", "feat: stable baseline")
            commit_file(repo, "nightly.txt", "fix\n", "fix: nightly change")

            legacy_changelog = run_git(repo, "log", "v1.0.0..HEAD", "--pretty=format:%s")
            self.assertIn("feat: stable baseline", legacy_changelog)

            changelog = changelog_since_stable("v1.0.0", cwd=repo)
            self.assertIn("fix: nightly change", changelog)
            self.assertNotIn("feat: stable baseline", changelog)


if __name__ == "__main__":
    unittest.main()

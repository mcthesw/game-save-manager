---
title: Contributing
---

## Submit a change

1. Open an issue describing the problem. For small changes, you can state your intention to submit a PR directly in the issue. Larger changes must be discussed with a maintainer first to agree on the direction
2. Fork the repository and create a branch from the latest `dev`
3. Address the issue, updating relevant tests and documentation
4. Run the [checks](./testing.md) and review your diff
5. Push your branch and open a PR targeting the upstream `dev` branch, linking the issue

Explain the problem and resulting behavior in the PR description. Include screenshots for interface changes.

## Commit messages

Use English [Conventional Commits](https://www.conventionalcommits.org/) with the format `type(scope): :emoji: summary`:

```text
fix(backup): :bug: handle missing snapshot files
docs(guide): :memo: clarify the restore steps
```

Keep changes with different purposes in separate commits. Rebase is the usual merge method; small changes may use squash.

## Keep your branch up to date

Add the upstream repository once:

```bash
git remote add upstream https://github.com/mcthesw/game-save-manager.git
```

On your feature branch:

```bash
git fetch upstream
git rebase upstream/dev
```

Resolve conflicts and rerun relevant checks. Maintainers handle version numbers and releases.

## Documentation and translations

- [Improve documentation](../contribute/document.md) with concise steps describing current behavior
- [Translate the application](../contribute/translate.md) through Weblate
- Chinese developer guides live in `apps/rgsm-docs/docs/developers`; English guides live in the matching `i18n/en/docusaurus-plugin-content-docs/current/developers` directory

# Releasing RGSM

RGSM uses one stable application version across Windows, Linux and macOS.
Candidate builds use that same version and contain every regular package type;
they are Actions artifacts, not GitHub prereleases or public RC tags. Because
the repository is public, these artifacts are not a secrecy boundary.
Nightly builds remain separate.

## Prepare and test a candidate

1. Run **Prepare Release** on `dev` with the next stable version, such as `1.9.0`
2. Review the generated version PR, release notes and ordinary CI checks
3. Run **Build Candidate** with that open release PR's number
4. Download the packages from the workflow run and test the platforms you intend to support, including an upgrade from the previous release

If acceptance finds a problem, fix it on `dev`, update the open version PR and
build another candidate from its new head. No version increment or public tag
is needed. The workflow resolves the PR head once, checks the version files,
then builds all platforms from that exact commit. The first version under this
process is selected explicitly; the release history starts at `v1.8.0`.

`RELEASE_TOKEN` must be a repository token able to create branches and PRs so
the generated PR triggers normal checks. Candidate builds use the built-in
`GITHUB_TOKEN` and do not create a GitHub Release.

## Publish

Merging the accepted version PR is the publication approval. Release Please
creates an immutable tag and draft GitHub Release. All platform packages are
built again from the tagged commit. The release becomes public and Latest only
when the required checks pass and every expected package is attached.

The final packages are rebuilt from the tagged commit; candidate and final
artifacts are not assumed to be byte-identical. A failed run leaves a draft.
Fix a source problem before merging the version PR; for a failure after tagging,
retry the build or run **Publish Release** with the existing draft tag. Do not
move an existing tag or rebuild a published version.

The preparation wrapper requests a specific version from Release Please even
when no new user-facing commit exists. It does not need a dummy feature or fix
commit. The publishing jobs use the built-in `GITHUB_TOKEN` and call the
reusable build workflow directly, without depending on a second tag-push event.

## Configuration compatibility

The application version comes from the workspace `Cargo.toml`. The persisted
configuration format uses `updater::versions::CURRENT_VERSION`; change it only
when the format changes. A candidate build must not migrate data merely because
the application version is being prepared for release.

Release Please uses its `simple` strategy because its Rust strategy does not
support inherited workspace versions. `.release-version` is its bookkeeping
file; explicit TOML updates keep the workspace version and its five lockfile
entries together without replacing `version.workspace = true` in member crates.

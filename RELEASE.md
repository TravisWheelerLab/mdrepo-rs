# Releasing `mdr-meta`

A release publishes **`mdr-meta` only**. `mdr-process` and `mdr-export` are
never released — they reach a host only by being built there
(`just mdr-process`), so a release never changes what the ingest pipeline or
the queue runs.

This matters more than it sounds: a fix committed to `main` is **inert until a
release**. No submitter sees it, and `mdr-meta check` on every deployed host
still refuses what the fix accepts. "Committed" is not "shipped".

## What the tag sets off

`.github/workflows/release.yml` fires on a pushed tag matching `v[0-9]*` and
does three things:

1. **Creates the GitHub release**, taking the body **verbatim from the matching
   `## <version>` heading in `mdr-meta/CHANGELOG.md`**. Write the entry
   *before* tagging — the heading must match the tag with the `v` stripped.
   A missing entry does not fail the release (`allow-missing-changelog: true`),
   it just publishes an empty body, which is worse than it sounds because
   nobody notices.
2. **Builds and uploads `mdr-meta`** for `x86_64` Linux, macOS and Windows
   (`.tar.gz` on unix, `.zip` on windows).
3. **Updates the release table** via `.github/workflows/update_release_table.py`.

## The procedure

Every release points at a commit **on `main`**. Tag the commit you pushed, not
a local one.

```bash
# 0. main, clean, up to date, and green
git switch main && git pull --ff-only
cargo test --locked -p libmdrepo -p mdr-meta

# 1. Write the changelog entry FIRST -- this becomes the release note
$EDITOR mdr-meta/CHANGELOG.md          # add "## <version>" at the top

# 2. Bump the version
$EDITOR mdr-meta/Cargo.toml            # version = "<version>"
cargo update -p mdr-meta --precise <version>    # refresh Cargo.lock

# 3. Prove the bump built
cargo test --locked -p libmdrepo -p mdr-meta
cargo run --locked -q -p mdr-meta -- --version  # must print the new version

# 4. Commit and push to main
git add mdr-meta/CHANGELOG.md mdr-meta/Cargo.toml Cargo.lock
git commit -m "mdr-meta <version>"
git push origin main

# 5. Tag THAT commit and push the tag
git tag v<version>
git push origin v<version>
```

Then watch the action, and **verify the published artifact rather than assuming
it**: download the asset from the release's own URL — ideally the one the
`md-repo-app` Dockerfile pins — and check it reports the new version.

## Before you tag

- **Is the change additive to an allow-list, or a tightening?** Additive
  (accepting something previously refused) cannot make a passing file fail, so
  the whole-corpus gate is not needed. A **tightening** — `deny_unknown_fields`,
  a new required key, a narrowed enum — can fail files that pass today, and
  must be run over the whole released corpus first.
- **The whole-corpus gate** is both builds over the ~97,809 released TOMLs in
  `/opt/mdrepo/toml/prod-release`, stdout captured and diffed. It lives **on the
  ops box** — the corpus is not on the ddd host, so this cannot be run there.
- **Does `md-repo-app` need re-pinning?** Its Dockerfile pins an `mdr-meta`
  version, so the site's validator does not move until someone bumps it and the
  pods rebuild. A release alone does not reach the website.
- **Check what else rides along.** A release ships everything on `main`, not
  just the change you have in mind. `git log --oneline <lasttag>..HEAD
  --name-only -- mdr-meta libmdrepo` shows what actually affects the binary;
  changes under `mdr-process/` do not.
- **Version numbers are cheap but not free.** Travis has asked before that a
  version not be spent on a change nobody is waiting for.

## History

Reconstructed from the 2026-09-04 diary entry on each of the last few releases,
because this file was an unedited template until 0.3.25. If the procedure
changes, change it here.

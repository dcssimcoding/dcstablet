# Repository privacy

Apply these rules before publishing commits, tags, pull requests, comments, or
release assets. Re-read this file at the start of each session in this checkout.

## Commit and tag identities

- Use this identity for both commit authors and committers:
  `dcssimcoding <350708904+dcssimcoding@users.noreply.github.com>`.
- Set the identity in this repository's Git configuration, not the global
  configuration. Do not rely on inherited account or machine defaults.
- Before pushing, inspect every outgoing commit's author and committer names and
  email addresses, not just the latest commit. Inspect annotated tagger metadata
  as well. Keep the noreply identity even when amending or rebasing commits.
- Do not silently rewrite another contributor's attribution. If their identity
  needs changing, resolve that with them before publishing the affected commits.
- Use GitHub-generated merges only when the resulting author and committer
  identities are known to use noreply addresses. Otherwise, create the merge
  locally with the configured identity. Verify the published metadata afterward.
- Do not assume GitHub email privacy settings or push blocking validate every
  historical commit. Verify the metadata independently.

## Files and release assets

- Keep credentials, private backups, local configuration, personal filesystem
  paths, and private audit findings outside tracked files and public messages.
  Do not document protected personal values as examples or test fixtures.
- Review the complete outgoing diff for identifying information and accidental
  files. Preserve legitimate third-party license notices and attribution.
- Before compiling release dependencies, run `scripts/Set-PrivateBuild.ps1` in
  the build process to configure path remapping. Follow `CONTRIBUTING.md` for
  rebuilding an existing checkout with these flags.
- Run `scripts/Check-ReleasePrivacy.py` on the executable and complete release
  ZIP. Use `scripts/Package.ps1`, which enforces these checks. Also review build
  metadata and other packaged files for unintended identity details; a passing
  path scan is not a complete identity audit.
- If any privacy check fails, stop publication and correct the finding before
  retrying. Avoid reproducing the protected value in logs or public explanations.

## Scope of work

- A request to commit documentation does not authorize tags, releases, builds,
  or new release assets. For documentation-only work that needs no CI, use
  `[skip ci]` in the commit message to avoid triggering the packaging workflow.
- These rules apply to this repository. They do not establish account-wide
  memory or verify settings in a different checkout or a future session.

# Release + distribution runbook

This document describes how to cut a Mnemos release.

## Release flow (summary)

1. Land all PRs targeting the release. Run the full test suite locally:

   ```
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   (cd desktop && pnpm typecheck && pnpm lint && pnpm test -- --run && pnpm build)
   ```

2. Update `CHANGELOG.md`: add a `## [X.Y.Z] - YYYY-MM-DD` block at the
   top describing what's new.

3. Bump versions in:
   - `Cargo.toml` `[workspace.package] version`
   - `desktop/package.json` `version`
   - `desktop/src-tauri/Cargo.toml` `version`
   - `desktop/src-tauri/tauri.conf.json` `version`

4. Commit the bump:

   ```
   git add Cargo.toml desktop/package.json desktop/src-tauri/Cargo.toml \
           desktop/src-tauri/tauri.conf.json CHANGELOG.md
   git commit -m "chore: release vX.Y.Z"
   ```

5. Tag:

   ```
   git tag -a vX.Y.Z -m "vX.Y.Z — short summary"
   ```

6. Push:

   ```
   git push origin master --tags
   ```

7. Watch the GitHub Actions "Release" workflow. On success, the release
   appears at `https://github.com/UnpaidAttention/mnemos/releases/tag/vX.Y.Z` with:
   - `Mnemos_X.Y.Z_aarch64.dmg`, `.app.tar.gz`, `.app.tar.gz.sig`
   - `Mnemos_X.Y.Z_amd64.AppImage`, `.AppImage.tar.gz`, `.AppImage.tar.gz.sig`
   - `Mnemos_X.Y.Z_amd64.deb`, `mnemos-X.Y.Z-1.x86_64.rpm`
   - `Mnemos_X.Y.Z_x64_en-US.msi`, `.msi.zip`, `.msi.zip.sig`
   - `mnemos_X.Y.Z_amd64.deb` (server-side CLI)
   - `mnemos-daemon_X.Y.Z_amd64.deb`
   - `.rpm` equivalents
   - `latest.json` — Tauri updater manifest

8. Verify a download:
   - **macOS**: open the `.dmg`, drag to Applications, launch, confirm the
     `Mnemos` window opens, run a quick `mnemos --version` from a terminal
     pointed at the bundled binary, smoke-test the Doctor view.
   - **Linux**: `sudo dpkg -i Mnemos_*.deb` AND `sudo dpkg -i mnemos_*.deb` (the
     CLI-only one). Run `mnemos --version`, run the bundled daemon via the
     `.desktop` entry, smoke-test the GUI.
   - **Windows**: install the `.msi`, launch from the Start Menu, smoke-test.

## Dry-running the release workflow

The `release.yml` workflow exposes a `workflow_dispatch` trigger with a
`tag` input. To validate without cutting a real release:

```
gh workflow run "Release" -f tag=v0.7.0-rc1
gh run watch
```

(Create a `v0.7.0-rc1` tag locally first, push it, then dispatch.) After
the run, delete the `v0.7.0-rc1` release + tag.

## Auto-update verification (post-release)

After a release publishes:

1. Take a known-older Mnemos build (e.g., the previous version's `.dmg`)
   and install it.
2. Launch it. The UpdateBanner should appear within ~5 seconds with the
   new version's number.
3. Click Install. Confirm the download progresses and the app prompts
   to relaunch.
4. Relaunch and verify the version in `About` matches.

If the banner never appears:

- Check the release's `latest.json` exists and is reachable:
  `curl -sSf https://github.com/UnpaidAttention/mnemos/releases/latest/download/latest.json`.
- Check the older build's `tauri.conf.json` updater endpoint matches.
- Check the older build's public key matches the current signing key
  (if you rotated, older builds are stranded — they need a manual
  download).
- Check that `tauri.conf.json → plugins.updater.pubkey` is NOT the
  placeholder string `PLACEHOLDER_PUBLIC_KEY_REPLACE_BEFORE_RELEASE`
  (that's the pre-release sentinel; a real signed release replaces it).

## Linux package repositories

`.deb` and `.rpm` artifacts on GitHub Releases are user-installable via
`dpkg -i` / `rpm -i`, but for `apt`/`dnf` to find them automatically,
they need to live in a repository.

### apt: Launchpad PPA

1. [Create a Launchpad account](https://launchpad.net) and a PPA, e.g.
   `~unpaidattention/+archive/ubuntu/mnemos`.
2. Generate a GPG key tied to that account.
3. Sign each `.deb` with the key:

   ```
   debsign -k <KEYID> *.deb
   ```

4. Upload:

   ```
   dput ppa:unpaidattention/mnemos *.changes
   ```

Launchpad rebuilds the source package and signs the resulting binary.
Users then add the PPA:

```
sudo add-apt-repository ppa:unpaidattention/mnemos
sudo apt update
sudo apt install mnemos mnemos-daemon
```

### dnf: openSUSE Build Service (OBS)

OBS is the most universal RPM hosting layer (Fedora COPR is the
alternative — choose based on your audience).

1. Create an OBS account at <https://build.opensuse.org>.
2. Create a project (e.g. `home:UnpaidAttention:mnemos`).
3. Per release, upload the `.rpm` (or the source `.spec` + tarball).
4. OBS rebuilds against Fedora, openSUSE, RHEL etc. and serves a repo.
5. Users add the repo and install:

   ```
   sudo dnf config-manager --add-repo \
     https://download.opensuse.org/repositories/home:UnpaidAttention:mnemos/Fedora_38/home:UnpaidAttention:mnemos.repo
   sudo dnf install mnemos mnemos-daemon
   ```

> Both Launchpad and OBS require accounts the framework cannot
> auto-create. Treat this section as the manual follow-up after the
> first release lands.

### AppImage updates

AppImages don't have a package repo concept. The Tauri updater
serves the new AppImage via `latest.json` and replaces the existing
file in place.

## Code-signing

See [BUILD.md](BUILD.md) § "Code-signing" for the macOS and Windows
secret setup. Once the secrets are added, the next release runs
through CI and produces signed installers without code changes.

## Rotating the updater key

See [BUILD.md](BUILD.md) § "Tauri updater signing key" — only do this
after a key compromise. Users on the prior key won't auto-update.

## Common release-day issues

- **`actions/upload-artifact` fails: artifact too large** — Tauri
  bundles are ~80 MB compressed; if you're seeing >2 GB, check that
  `target/release/bundle/` isn't being globbed (only `staged/` should
  be uploaded). The workflow already scopes this.

- **`pnpm tauri build` runs but no `.app.tar.gz` is emitted** —
  confirm `tauri.conf.json` has `bundle.createUpdaterArtifacts: true`.
  Without it, Tauri skips the updater-specific tarballs.

- **Signature mismatch on update** — the public key in
  `tauri.conf.json` doesn't match the private key in CI. Confirm
  via `pnpm tauri signer sign --help` and a manual signature test
  before re-cutting.

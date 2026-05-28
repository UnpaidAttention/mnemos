## Mnemos $VERSION

See [CHANGELOG.md](https://github.com/UnpaidAttention/mnemos/blob/master/CHANGELOG.md) for what's new.

### Downloads

| Platform | File |
|---|---|
| macOS (Apple Silicon) | `Mnemos_${VERSION}_aarch64.dmg` |
| Linux (x86_64) | `Mnemos_${VERSION}_amd64.AppImage` / `mnemos_${VERSION}_amd64.deb` / `mnemos-${VERSION}-1.x86_64.rpm` |
| Windows (x86_64) | `Mnemos_${VERSION}_x64_en-US.msi` |

The macOS and Windows builds are **unsigned**. macOS may warn about an unidentified developer (right-click → Open the first time). Windows will SmartScreen-warn (More info → Run anyway).

Linux server / CLI-only packages: `mnemos_${VERSION}_amd64.deb` (CLI) and `mnemos-daemon_${VERSION}_amd64.deb` (daemon). RPM equivalents under `target/generate-rpm/`.

### Auto-update

The desktop app polls `https://github.com/UnpaidAttention/mnemos/releases/latest/download/latest.json` on launch. Update manifests are signed with the project's ed25519 key.

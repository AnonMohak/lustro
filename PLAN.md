# Lustro — Plan

Lustro is a Windows 11 storage manager. It scans drives, shows what uses the space, and helps you remove what you do not need.

## Status

Milestones M0 to M10 are done. Lustro v1.0.0 is published on [GitHub Releases](https://github.com/AnonMohak/lustro/releases) and tested with and without admin rights. The [README](README.md) describes the features. The git history and the closed GitHub milestones record the work.

## Facts for later work

- **Updater signing key:** `%USERPROFILE%\.tauri\lustro-updater.key` (back it up) and the `TAURI_SIGNING_PRIVATE_KEY` repo secret. Without this key, installed copies cannot update.
- **Release steps:** change the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`. Push a tag such as `v1.0.1`. The release workflow makes a draft release with `latest.json`. Publish the draft; installed copies then update.
- **Logo file:** `src/assets/logo.svg`. Run `npm run icons` after a change.

## Open risks

| Risk | Plan |
|---|---|
| SmartScreen warns about the unsigned installer | M12 (code signing). Until then, the README tells users what to do. |
| Antivirus marks the app (raw disk read and delete) | M12 helps. Submit false-positive reports when it happens. |

---

## Extra work (optional)

These milestones are not part of v1.0.0. Do them in this order, when you want them.

### M11 — Real logo

- You provide the logo as an SVG file (or a PNG of at least 1024 × 1024 pixels).
- Replace `src/assets/logo.svg` and run `npm run icons`. This makes the window, tray, taskbar and installer icons.
- Check the logo at 16 px (tray and title bar) and on light and dark backgrounds.
- Release v1.0.1. Installed copies get the new logo through the auto-update.
- **Exit:** the new logo shows in the app, the tray, the Start menu and the installer.

### M12 — Code signing

- Choose a signing option:
  - **Azure Trusted Signing** (recommended): about $10 per month. Needs an identity check (individual or company).
  - **OV or EV certificate**: about $200–400 per year. Needs a hardware token or a cloud HSM.
- Add the signing step to the release workflow: sign `lustro.exe` and the NSIS installer. The secrets go into GitHub Actions.
- Remove the SmartScreen note from the README and the release text when the reputation builds up.
- **Exit:** the installer and the app show your name as the publisher. SmartScreen stops warning (it can take some downloads before the reputation builds up).

### M13 — Windows 11 top-level right-click menu

- Needs M12: Windows accepts the top-level menu only from a signed package.
- Add an `IExplorerCommand` handler (a small COM DLL) and a sparse MSIX package that gives Lustro a package identity.
- The installer registers the sparse package; the uninstaller removes it.
- Keep the classic menu entry ("Show more options") as a fallback.
- **Exit:** "Analyze with Lustro" shows in the first Windows 11 right-click menu of folders and drives, without "Show more options".

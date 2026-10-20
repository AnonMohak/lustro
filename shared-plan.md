# Lustro on Linux and macOS

This plan ports Lustro from Windows 11 to Linux and to macOS on Apple silicon. Windows stays the main platform. Each phase must keep Windows behavior the same.

## Start point

- The Rust backend has about 7,700 lines. Before Phase 1, it had no `cfg(windows)` gates.
- 13 modules call the `windows` crate directly: `scan/mft.rs`, `scan/usn.rs`, `scan/walk.rs`, `apps/mod.rs`, `clean/mod.rs`, `clean/dev.rs`, `clean/system.rs`, `commands.rs`, `dupes/mod.rs`, `error.rs`, `ops/delete.rs`, `system/drives.rs` and `system/elevation.rs`.
- `Cargo.toml` had `windows` and `winreg` in the shared dependencies, so the crate did not compile on other targets.
- The window uses `acrylic` effects and a custom title bar.
- CI and release run only on `windows-latest`. The `setup/` installer is NSIS only.

## Phase 1: Platform layer (Windows only, no new features)

1. Move `windows` and `winreg` into `[target.'cfg(windows)'.dependencies]`.
2. Put the OS-specific code behind `cfg` gates, with one interface for each area: drives, elevation, protected paths, fast scan, live updates, apps, clean rules, desktop integration and the window effect.
3. Keep the Windows code. Its behavior must not change.
4. Add stubs for Linux and macOS, so that `cargo check` passes for both targets. A stub returns an empty list or a clear "not supported" error.
5. Add CI jobs on `ubuntu-latest` and `macos-14` that compile the backend and run the tests.

This phase has low risk because Windows stays the same.

### Phase 1 status

Done on branch `cross-platform`:

- `windows` and `winreg` are Windows-only dependencies.
- Windows-only modules have `cfg(windows)`: `scan/mft.rs`, `scan/usn.rs`, `scan/walk.rs`, `live.rs` and `clean/system.rs`. Each has an inline stub module for other targets.
- Windows-only functions have `cfg(windows)` and a stub: `system/drives.rs`, `system/elevation.rs`, `apps/mod.rs` and `settings.rs`.
- Shared helpers in `system/mod.rs`: `file_attributes` (the Windows attribute bits from any `Metadata`) and `no_window` (`CREATE_NO_WINDOW` on Windows only). `clean/mod.rs`, `ops/delete.rs` and `clean/dev.rs` use them.
- `dupes` gets the file identity from `dev`+`ino` on Unix.
- `reveal_path` uses `open -R` on macOS and `xdg-open` on Linux.
- History restore uses `trash::os_limited` on Windows and Linux. On macOS the stub reports that nothing is in the Trash.
- `lib.rs` allows dead code on non-Windows targets until the stubs become real code. Remove this line in Phases 2 and 3.
- CI job `unix-check` runs `cargo clippy --all-targets` on `ubuntu-22.04` and `macos-14`.

Not done yet:

- Tests do not run on Linux and macOS. Many tests use Windows paths (`C:\`, `\`), and the tree code joins paths with `\`. Phase 2 makes paths portable and adds `cargo test` to `unix-check`.
- On Linux and macOS the app starts, but it has no drives and scans fail with "not available".

### Check other targets on Windows

`cargo clippy --target aarch64-apple-darwin` and `--target x86_64-unknown-linux-gnu` work on Windows with a stand-in C compiler and pkg-config: a small program that writes an empty file at `-o` and prints a version for `--modversion`. Set `CC_<target>`, `AR_<target>=llvm-ar`, `PKG_CONFIG` and `PKG_CONFIG_ALLOW_CROSS=1`. This only checks the code: it cannot link or run it.

## Phase 2: Linux

| Area | Linux approach |
|---|---|
| Scan | Use `walk.rs` with `dev`+`inode` to count hard links once. Stay on one filesystem. Skip `/proc`, `/sys`, `/dev` and `/run`. There is no MFT equivalent, so a parallel walk is the fast path. |
| Live updates | Start with "rescan only". Later, add `fanotify` (needs root) or `inotify` (has watch limits). |
| Drives | Read `/proc/self/mountinfo` and use `statvfs`. Remove pseudo, snap and loop filesystems. |
| Admin | Relaunch with `pkexec`, or start without elevation. |
| Protected paths | `/`, `/usr`, `/etc`, `/boot`, `/var/lib`, `$HOME` itself and the XDG folders. |
| Delete | The `trash` crate supports the freedesktop trash, with list and restore. |
| Clean | `~/.cache`, thumbnails, apt/dnf/pacman caches, old snap revisions, unused flatpak runtimes, `journalctl --vacuum`. Dev caches stay, with other paths. |
| Apps | Read dpkg/rpm/pacman, flatpak, snap and `.desktop` files. Uninstall goes through the package manager. Start as read-only. |
| Integration | Tray needs `libappindicator`. Autostart uses `~/.config/autostart`. Skip the file manager menu at first. |
| UI | There is no acrylic, so use the solid fallback. Test window drag on Wayland and WebKitGTK speed on the treemap. |
| Packages | AppImage (the updater supports it), `.deb` and `.rpm`. |

## Phase 3: macOS (Apple silicon)

| Area | macOS approach |
|---|---|
| Scan | Use `getattrlistbulk` for a fast walk. Prevent double counts from the `/System/Volumes/Data` firmlinks and from APFS clones. |
| Live updates | FSEvents. It needs no root. |
| Permissions | macOS uses Full Disk Access (TCC), not admin. Detect missing access and open System Settings. |
| Drives | APFS volumes share one container, so show container space. Also show purgeable space and local Time Machine snapshots. |
| Protected paths | SIP paths (`/System`, `/usr` except `/usr/local`), system apps and `~/Library` itself. |
| Delete | The Trash works. The `trash` crate gives list and restore only on Windows and Linux, so macOS needs its own restore code. |
| Clean | `~/Library/Caches`, logs, Xcode `DerivedData`, old simulators, iOS backups, the Homebrew cache. |
| Apps | Read `.app` bundles in `/Applications`. Get last use from Spotlight (`kMDItemLastUsedDate`). Uninstall moves the bundle and its `~/Library` leftovers to the Trash. |
| UI | Vibrancy and an overlay title bar with the traffic lights. Cmd shortcuts. A menu bar item in place of the tray. |
| Packages | `.dmg`, plus `.app.tar.gz` for the updater, for target `aarch64-apple-darwin`. |
| Signing | Ad-hoc signature, no Apple certificate (see below). |

### macOS signing: ad-hoc, no certificate

Lustro uses an ad-hoc signature. It costs nothing and needs no Apple Developer account. Apple silicon does not run code that has no signature, so an ad-hoc signature is the minimum.

1. In `tauri.conf.json`, set `bundle > macOS > signingIdentity` to `"-"`. Tauri then runs `codesign --sign -` on the app. The CI can also set `APPLE_SIGNING_IDENTITY=-`.
2. Do not notarize. Notarization needs a Developer ID certificate.
3. The updater signature (minisign, `TAURI_SIGNING_PRIVATE_KEY`) is a different signature. It stays the same as on Windows.
4. Check the result on a Mac with `codesign --verify --deep --strict Lustro.app`. `spctl --assess` must report "rejected". This is the expected result for ad-hoc signing.

What the user sees, and what the README must tell them:

- The first start of a downloaded app is blocked, because Gatekeeper cannot check the developer. On macOS 15 and later, the user opens System Settings > Privacy & Security and clicks **Open Anyway**. On older versions, the user right-clicks the app and chooses **Open**.
- As an alternative, the user runs `xattr -dr com.apple.quarantine /Applications/Lustro.app` one time.
- If macOS says that the app "is damaged", the signature is broken (for example, a step changed a file after `codesign`). Fix the build. Do not tell users to ignore it.

**Caution:** Full Disk Access belongs to the exact signature of an ad-hoc app. Each update has a new signature, so macOS can forget the grant after an update. Test this on a Mac. If it occurs, Lustro must detect the missing access after an update and send the user to System Settings again.

**Caution:** Updates that the Tauri updater downloads probably get no quarantine flag, so they start without a Gatekeeper block. Test this before the first macOS release.

A Developer ID certificate and notarization stay optional for later. They remove the Gatekeeper block and keep Full Disk Access across updates.

## Phase 4: Shared work

- **Frontend:** Remove the Windows assumptions in `drives.ts`, `admin.ts`, the path display and `en.json` (drive letters, `\`, "Program Files"). Get the OS from the backend and hide features that an OS does not have.
- **Release:** Change `release.yml` to a matrix. `latest.json` must list all platforms. The `setup/` installer stays Windows only.
- **Branding:** Change "Storage manager for Windows 11" in `Cargo.toml`, `tauri.conf.json`, the README and the app text.

## Phase 5: Android

Tauri 2 builds Android apps. The Rust backend runs as a native library in the app, and the UI runs in the Android WebView. Android uses the Linux kernel, so the Unix walk from Phase 2 also works here. The Android permission and storage model is very different, so Android is a separate product shape, not a copy of the desktop app.

### What Android allows

| Desktop feature | Android |
|---|---|
| Scan a drive | Shared storage (`/storage/emulated/0`, SD cards, USB drives) only. It needs the "All files access" permission (`MANAGE_EXTERNAL_STORAGE`). Without root, the app cannot read the private data of other apps (`/data/data`), and since Android 11 it cannot read `Android/data` and `Android/obb`. |
| Fast scan (MFT) | Not possible. Use the parallel walk from Phase 2. |
| Live updates | `FileObserver` (inotify) for open folders only. Start with "rescan only". |
| Drives | `StorageManager` storage volumes: internal storage, SD card, USB OTG. |
| App sizes | `StorageStatsManager` gives app, data and cache size for each app. It needs "Usage access" (`PACKAGE_USAGE_STATS`), which the user turns on in Settings. |
| Clean other apps' caches | Not possible since Android 6. Lustro shows the cache size and opens the app's settings page, where the user taps "Clear cache". |
| Clean rules | New Android rules: old APKs in `Download`, `.thumbnails`, empty folders, large and old media, messenger media in `Android/media`. Most Windows rules do not apply. |
| Duplicates | Works as on desktop (size, then BLAKE3). |
| Delete | With "All files access", a direct delete works. For media, use `MediaStore.createTrashRequest` (Android 11 and later), so that the user can restore items for 30 days. |
| Uninstall | Start the system uninstall screen (`ACTION_DELETE`, permission `REQUEST_DELETE_PACKAGES`). |
| Admin, tray, start with Windows, Explorer menu | Not applicable. Remove them from the Android UI. |
| Export | Save the CSV and HTML files through the Android file picker. |

### Work items

1. **Entry point:** Add `#[cfg_attr(mobile, tauri::mobile_entry_point)]` to `run()` in `lib.rs`. The crate already builds as `cdylib`.
2. **Desktop-only plugins:** Put the tray, `tauri-plugin-single-instance`, `tauri-plugin-updater` and `tauri-plugin-process` behind `cfg(desktop)`. The dialog and notification plugins support Android.
3. **Android plugin (Kotlin):** Write one Tauri mobile plugin for the Android APIs: permission requests, storage volumes, `StorageStatsManager`, the installed app list, the uninstall screen, the app settings page and the MediaStore trash. The Rust backend calls it through the Tauri plugin bridge.
4. **Permissions screen:** On the first start, explain and request "All files access", then "Usage access". The app must work with less access: without "All files access" it scans only folders that the user picks.
5. **Scan code:** Reuse the Unix walk and the portable paths from Phase 2. Add `cfg(target_os = "android")` where Android differs from Linux (protected paths, clean rules, no `/proc/self/mountinfo` drives).
6. **Mobile UI:** This is the largest item. The desktop layout needs a width of 960 px or more. Android needs:
   - a bottom navigation bar in place of the sidebar,
   - one pane at a time in place of tree, treemap and details next to each other,
   - touch gestures for the treemap (tap to zoom, back to zoom out),
   - no hover tooltips,
   - the Android back button.
7. **Updates:** The Tauri updater does not support Android. Use Google Play updates, or a check that opens the GitHub release page.
8. **Build and CI:** Install Java 17, the Android SDK and the NDK, and the Rust targets `aarch64-linux-android`, `armv7-linux-androideabi` and `x86_64-linux-android` (for the emulator). Run `npm run tauri android init` one time, and then `npm run tauri android build`. Add an Android job on `ubuntu-latest` to CI and to `release.yml`.

### Signing and distribution

- An Android app must have a signature, but you make the key yourself with `keytool`. It costs nothing. Keep the keystore and its password in GitHub secrets.
- **Warning:** Back up the keystore. If you lose it, you cannot publish updates to the same app.
- **GitHub releases (APK):** This is the easiest start. Users allow "Install unknown apps" one time.
- **Google Play:** The account costs a one-time fee. Play reviews "All files access" and the full app list (`QUERY_ALL_PACKAGES`). A storage manager can qualify, but you must send a declaration, and Play can refuse it.
- **F-Droid:** It is free and open source friendly. F-Droid builds the app from source with its own key. Not chosen for now.
- **Chosen:** GitHub releases and Google Play. A user can move between the APK and the Play version without an uninstall only when both have the same signature. New Play apps use Play App Signing, where Google signs the app. When you set up Play App Signing, give Google your own key (do not let Google make a new one). Then the GitHub APK and the Play version have the same signature.

### Tests

- Use the Android emulator on the Windows PC (Android Studio) for most tests.
- Test on one real phone before a release. Emulators do not show real storage speed, SD cards or the vendor's own storage rules.

## Order

1. Phase 1. Release it as Windows only.
2. Phase 2 (Linux). Test in a VM or in WSLg on the Windows PC.
3. Phase 3 (macOS). Test on a real Mac. CI alone cannot prove TCC, APFS and signing.
4. Phase 4 runs next to Phases 2 and 3.
5. Phase 5 (Android) starts after Phase 2 and the frontend part of Phase 4, because it reuses the Unix walk and the portable paths.

## Open questions

1. Google Play can refuse "All files access". What does Lustro do then? (Decide later, before the first Play release.)

## Decisions

- **macOS signing:** Ad-hoc signature, no Apple certificate.
- **macOS tests:** No Mac is available. CI on `macos-14` checks the build only. The TCC, APFS, Gatekeeper and Full Disk Access items in Phase 3 stay untested until someone tests on a real Mac. Mark the first macOS release as a preview.
- **Feature parity:** Not needed. Each platform gets the features that fit it. A feature that a platform does not have is hidden in the UI on that platform (for example the Explorer menu, the MFT scan and admin mode).
- **Linux distros:** Ubuntu and Debian first (`.deb` and AppImage). Other distros come later.
- **Android distribution:** GitHub releases (APK) and Google Play.
- **Android minimum version:** Android 11 (API 30). Set `minSdkVersion` to 30.

## Next steps

1. Push `cross-platform` and let CI run `unix-check` on Linux and macOS.
2. Merge into `master` when CI passes. Windows behavior does not change.
3. Install a Linux test system: run `wsl --install -d Ubuntu` (needs admin rights), or use a Linux VM.
4. Start Phase 2 with portable paths: the tree code, `normalize_root` and the tests use `C:\` and `\`. Everything else in Phase 2 depends on this.
5. Then add the Unix folder walk, the drive list from `/proc/self/mountinfo`, and `cargo test` in `unix-check`. Remove the dead-code line in `lib.rs`.
6. Then port trash and restore, clean rules, protected paths, the app list and autostart for Linux.

<p align="center"><img src="branding/lustro-logo.svg" width="128" height="128" alt="Lustro logo"></p>

# Lustro

Lustro is a free and open source storage manager for Windows 11, with previews for Linux and macOS. It scans your drives, shows what uses the space, and helps you remove what you do not need.

<p align="center">
  <img src="docs/demo.gif" width="800" alt="Lustro: fast scan, Explore, large files, duplicates, clean up, search and safe delete">
</p>

*Lustro* is Latin. It means "I purify" and also "I survey".

![Explore: folder tree, treemap and details](docs/screenshots/explore-dark.png)

## Features

- **Fast scan.** As administrator, Lustro reads the NTFS Master File Table directly: 2.2 million files in about 13 seconds. Without admin rights, it walks the folders.
- **Explore.** A folder tree sorted by size, a treemap that you can zoom into, a file type view, and a detail pane.
- **Large and old files.** The biggest files and the files that nobody changed for more than a year, over all scans.
- **Duplicates.** Three modes: exact copies (same size and BLAKE3 hash; hard links count once), files with the same name (also copy names like `photo (1).jpg` or `report - Copy.docx`), and similar photos (the same photo resized or re-saved, with previews). Whole duplicate folders show as one row, and a folder pairs view shows which two folders share copies. Filters by type and folder, keep rules (oldest, newest, shortest path, largest, or a folder), and a hash cache that makes the next search fast. Copies go to the Recycle Bin, or become links to the copy that stays (hard links on Windows and Linux, clones on macOS). Lustro selects extra copies only of photos, videos, music, documents and archives outside program folders.
- **Clean up.** Temp files, Windows Update cache, crash dumps, browser caches, the Recycle Bin, and dev junk: old `node_modules` and `target` folders, npm/pip/Cargo/NuGet caches, Docker images, WSL disks, IDE caches. You review every item first.
- **App caches.** One row for each app cache folder, with its size: `~/Library/Caches` and sandboxed apps on macOS (macOS has no button for this), `~/.cache` and Flatpak apps on Linux, and Store and Electron app caches on Windows. Lustro empties the folders; apps make them again. Android and iOS do not let an app clear the caches of other apps.
- **Your own clean rules.** For example: in your projects folder, move `node_modules` to the Recycle Bin when nothing else in the project changed for 30 days, or move installers (`*.exe`, `*.dmg`, `*.iso`) in Downloads after 14 days. Run a rule by hand, or let Lustro run it at start, daily or weekly (also from the tray). History can restore what it moved.
- **Free space now.** One page with the biggest easy wins of all features (junk, app caches, the Recycle Bin, old installers, duplicates, large old files, cloud files, old folders), each with one button.
- **What grew.** Lustro compares each new scan with the last one and shows the folders that grew most. It can rescan by itself (daily, weekly or monthly) and tell you what grew. Home shows the used space of each drive over time and when it will be full at that rate.
- **Low space actions.** When a drive is low, Lustro can run the rules and safe clean-up items that you choose, then tell you what it freed.
- **Move to another drive.** Move a big folder (a game, a cache) to another drive; a link in the old place keeps apps working.
- **Cloud and compression.** Make big old OneDrive or iCloud Drive files online-only, and compress folders that nobody changed for a year (NTFS compression on Windows, an archive on macOS and Linux).
- **Ignore list.** Folders that Lustro never suggests, in every feature.
- **Command line.** `lustro scan`, `clean`, `dupes` and `rules`, with JSON output, for scripts and schedulers.
- **Installed apps.** Size, install date and last use of desktop and Store apps. Uninstall runs the app's own uninstaller.
- **Search.** Names in all scans, with `*` and `?` wildcards. 2 million names in less than 0.1 seconds.
- **Live updates.** As administrator, the NTFS change journal keeps fast scans current without a rescan.
- **Safe delete.** Recycle Bin by default, a history with restore, and protected system paths that Lustro never deletes.
- **Windows integration.** Tray icon, low space alert, start with Windows, and "Analyze with Lustro" in the File Explorer menu (all optional).
- **Export.** CSV lists and an HTML report of a scan.

### On Linux

Linux support is a preview. Most features work as on Windows, with these differences:

- The scan walks the folders. There is no MFT scan and no live update, so rescan to see changes. The scan stays on one file system and skips `/proc`, `/sys`, `/dev` and `/run`.
- Delete moves items to the Trash. History restores them from the Trash.
- Clean up has Linux rules: thumbnails, shader caches, the Trash, apt packages, old snap revisions, old journal files, `/var/tmp`, crash reports, browser caches and dev caches.
- Installed apps lists deb packages with a menu entry, snaps and Flatpak apps. Lustro does not uninstall them. It shows the command to run.
- Start at login uses `~/.config/autostart`. There is no file manager menu entry.

### On macOS

macOS support is a preview for Macs with Apple silicon. Nobody tested it on a Mac yet. The differences from Linux:

- The scan of the system volume includes the Data volume (`/Users`, `/Applications`, ...) once. Many folders in `~/Library` need **Full Disk Access** (System Settings > Privacy & Security). Without it, Lustro skips them.
- Clean up has macOS rules: logs and crash reports, Quick Look thumbnails, iPhone backups and update files, browser caches, Xcode DerivedData, device support files and old simulators, and the Homebrew and dev caches.
- Installed apps lists the apps in `/Applications` and `~/Applications`, with the last use from Spotlight. Uninstall moves the app and its data in `~/Library` to the Trash.
- History restores an item only when the Trash still has it under its own name.
- Start at login uses a launch agent. There is no Finder menu entry.

| Light theme | Large and old files | Search |
|---|---|---|
| ![Light theme](docs/screenshots/explore-light.png) | ![Large and old files](docs/screenshots/large-files.png) | ![Search](docs/screenshots/search.png) |

## Install

### Windows

Download `Lustro-Setup-x.y.z.exe` from [Releases](https://github.com/AnonMohak/lustro/releases) and run it. The installer needs no admin rights. You can choose the folder, a desktop shortcut, start with Windows and the Explorer menu entry. Lustro updates itself from GitHub Releases (you can turn this off in Settings).

The release also has `Lustro_x.y.z_x64-setup.exe`. It is the update package that Lustro downloads by itself.

> **SmartScreen warning.** The installer is not code-signed yet, so Windows may show "Windows protected your PC". Choose **More info**, then **Run anyway**.

### Linux (preview)

Lustro is built and tested for Ubuntu and Debian (x86-64). Download one of these files from [Releases](https://github.com/AnonMohak/lustro/releases):

- **`.deb`** (Ubuntu and Debian): `sudo apt install ./Lustro_x.y.z_amd64.deb`. To update, install the new `.deb` the same way.
- **`.AppImage`** (other distros too): run `chmod +x Lustro_x.y.z_amd64.AppImage`, then start the file. The AppImage updates itself from GitHub Releases.

The AppImage needs FUSE 2. On Ubuntu 24.04 and later, run `sudo apt install libfuse2t64`. On Ubuntu 22.04, run `sudo apt install libfuse2`.

The tray icon needs AppIndicator support. GNOME shows it only with the "AppIndicator and KStatusNotifierItem Support" extension (Ubuntu has it on by default).

### macOS (preview)

For Macs with Apple silicon and macOS 11 or later. Download `Lustro_x.y.z_aarch64.dmg` from [Releases](https://github.com/AnonMohak/lustro/releases), open it, and drag Lustro to Applications. Lustro updates itself from GitHub Releases.

> **Gatekeeper warning.** Lustro has an ad-hoc signature, not an Apple Developer ID, so macOS blocks the first start. On macOS 15 and later, open System Settings > Privacy & Security and choose **Open Anyway**. On older versions, right-click Lustro in Applications and choose **Open**. Or run `xattr -dr com.apple.quarantine /Applications/Lustro.app` one time.
>
> If macOS says that Lustro "is damaged", the download is broken. Download it again. Do not change the app.

To scan all of your home folder, give Lustro Full Disk Access: System Settings > Privacy & Security > Full Disk Access. After an update, macOS can forget this access, because each version has a new ad-hoc signature. Then give it again.

## Admin rights

Lustro starts as a normal user. Use **Run as admin** in the title bar to restart it with admin rights. You need them for:

- the fast MFT scan and live updates,
- folders that Windows hides from normal users (for example `System Volume Information`),
- the system junk in `C:\Windows` (Windows temp, Windows Update cache, Windows.old).

Everything else works without admin rights.

On Linux and macOS, admin rights mean that Lustro runs as root. Lustro cannot restart itself as root, so **Run as admin** is not available. Normally you do not need root: start Lustro as root only to scan or clean folders that your user cannot read. On macOS, Full Disk Access is usually enough.

## Safety

- Lustro never deletes protected paths: `C:\Windows`, `Program Files`, the page file, drive roots, user profile folders and similar. The backend checks every path before it deletes.
- One exception, off by default: as admin, with Settings > "Allow deleting app folders" on, you can delete app and game folders inside `Program Files`. Each delete shows a warning that you must confirm. Windows' own folders there (for example `WindowsApps`) stay protected.
- Delete moves items to the Recycle Bin. A permanent delete always asks first.
- Lustro removes junctions and symbolic links as links. It never deletes the files that they point to.
- The cleaner deletes caches and temp files permanently. Apps make them again.
- On Linux, the protected paths are `/`, the system folders (`/usr`, `/etc`, `/var`, `/boot`, ...) with their contents, `/home`, `/tmp`, the home folders and their standard folders, and `.ssh`, `.gnupg` and `.pki`. Apps in `/opt` work like apps in `Program Files`.
- On macOS, the protected paths are `/`, `/System`, `/usr`, `/opt`, `/private` (with `/etc` and `/var`), `/Library` and `/Applications` with their contents, the home folders and their standard folders, `~/Library`, `.ssh`, `.gnupg` and the keychains. The check ignores case, like macOS. The cleaner may delete inside `/Library/Caches`, `/Library/Logs` and the temp folders.

## Privacy

Lustro has no account, no telemetry and no ads. The only network request is the update check to GitHub, which you can turn off. Scans, history and settings stay in `%LOCALAPPDATA%\com.anonmohak.lustro` (on Linux, `~/.local/share/com.anonmohak.lustro`; on macOS, `~/Library/Application Support/com.anonmohak.lustro`).

## Build from source

You need Windows 11, [Rust](https://rustup.rs) (MSVC toolchain), Node.js 22 or later, and the Visual Studio C++ build tools.

```powershell
npm install
npm run tauri dev     # run the app in dev mode
npm run tauri build   # make the installer in src-tauri/target/release/bundle/nsis
npm run build:setup   # put it in the themed installer: src-tauri/target/release/bundle/setup
npm run setup:dev     # run the themed installer window in dev mode
npm test              # frontend tests
cd src-tauri; cargo test
cd ../setup/src-tauri; cargo test
```

The themed installer (`setup/`) is a small Tauri app. It holds the NSIS installer, runs it without a window, and then applies the options. A copy of it in the install folder is the uninstaller.

`npm run tauri build` also signs the update package, so it needs the updater key in `TAURI_SIGNING_PRIVATE_KEY`. Without the key, build with `npm run tauri build -- --config src-tauri/tauri.ci.conf.json`.

If the Rust build fails with `link.exe` errors, another `link.exe` (from Git, Cygwin or MSYS) is earlier in your `PATH` than the MSVC linker. Remove those folders from `PATH` in that terminal, or build from a "Developer PowerShell for VS".

Some tests need admin rights and are ignored by default. Run them in an elevated terminal:

```powershell
cd src-tauri
cargo test --profile perf -- --ignored usn_sees_new_file mft_matches_walk --nocapture
```

### On Linux

You need [Rust](https://rustup.rs), Node.js 22 or later, and the WebKitGTK build packages. On Ubuntu 22.04 or later:

```sh
sudo apt install build-essential libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev patchelf
npm install
npm run tauri dev
npm run tauri build -- --config src-tauri/tauri.ci.conf.json   # .deb and AppImage in src-tauri/target/release/bundle
cd src-tauri && cargo test
```

Tauri adds `src-tauri/tauri.linux.conf.json` to the config on Linux. The themed installer (`setup/`) is for Windows only.

### On macOS

You need [Rust](https://rustup.rs), Node.js 22 or later, and the Xcode command line tools (`xcode-select --install`).

```sh
npm install
npm run tauri dev
npm run tauri build -- --config src-tauri/tauri.ci.conf.json   # Lustro.app and the .dmg in src-tauri/target/release/bundle
cd src-tauri && cargo test
```

Tauri adds `src-tauri/tauri.macos.conf.json` and `src-tauri/Info.plist` on macOS. The build signs the app ad hoc (`signingIdentity` is `-`).

## Command line

The same `lustro` program runs without a window when the first argument is a command. It shares the scans, rules, hash cache, ignore list and History with the window app.

```sh
lustro scan D:\ --top 20          # scan, show the biggest items, save the scan
lustro clean --list               # clean-up items and their sizes
lustro clean --safe --dry-run     # what the safe items would free
lustro clean user-temp thumbnails # run two items (permanent delete)
lustro dupes D:\Photos --mode similar --min-size 64k
lustro rules run "Old node_modules" --dry-run
lustro scan D:\ --json            # JSON for scripts
```

`dupes` only reports. Exit codes: 0 ok, 1 error, 2 wrong arguments. On Windows, pipe the output (for example `lustro scan D:\ | more` or `> report.json`) so that the shell waits for it. Use it from Task Scheduler, cron or launchd for your own schedules.

## Screenshots

The README screenshots show a demo folder, not real user data. `scripts/make-demo.ps1 -Root "C:\Users\you\Lustro Demo"` makes that folder from sparse files: about 117 GB in size, but almost no space on the disk. Scan it with "Scan a folder", then delete it.

## Release

Push a tag like `v1.0.1`. The release workflow builds the Windows installer, the Linux `.deb` and AppImage, and the macOS `.dmg`, signs the update packages, and makes a draft release with `latest.json`. Publish the draft on GitHub; then installed copies update.

## Logo

The logo is an "L" of two tiles with a sparkle: the treemap and the clean-up in one mark. The source is `src/assets/logo.svg` (a copy is in `branding/lustro-logo.svg`). After a change, run `npm run icons`: the app, the window, the tray and the installer all use this one file. `branding/social-preview.png` is the GitHub social preview.

## Tech

Tauri 2, Rust, React, TypeScript. The open and optional work is in [`PLAN.md`](PLAN.md); the original design mockup is in [`mockup/index.html`](mockup/index.html).

## License

[MIT](LICENSE)

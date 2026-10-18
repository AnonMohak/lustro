# Lustro — Plan

Lustro is a Windows 11 storage manager. It scans drives, shows what uses the space, and helps you remove what you do not need.

*Lustro* is Latin. It means "I purify" and also "I survey".

Status: M0 to M9 are done. See section 4.

---

## 1. Decisions (from the questions)

| Topic | Decision |
|---|---|
| Stack | Tauri 2 + Rust backend + React + TypeScript frontend |
| Scan | Fast NTFS MFT read when admin. Parallel folder walk when not admin, and for non-NTFS drives. |
| Admin | The app starts as a normal user. A "Run as admin" button restarts it elevated. |
| Drives | Internal NTFS drives, external USB drives (exFAT/FAT32), any folder the user picks. No network drives. |
| Delete | Recycle Bin by default. Permanent delete with Shift+Delete or a toggle, after a confirm dialog. |
| Protection | Always block the delete of protected system paths. No override. |
| History | Log of all deletes. "Restore" button for items in the Recycle Bin. |
| Look | Windows 11 Fluent: Mica, rounded corners, Segoe UI Variable. |
| Theme | Follow the Windows light/dark setting. Manual switch in Settings. |
| Size chart | Treemap |
| Windows | Windows 11 only |
| Language | English. All UI text is in translation files. |
| Users | Public release. Free and open source. |
| Distribution | GitHub Releases + Tauri auto-update |
| Code signing | Later. The first releases are unsigned. |
| Repo | This folder + a new public GitHub repo `lustro`. Default branch: `master`. |
| Work order | HTML mockup first. Then 4 phases. You test each phase. |

### Features

- Drive overview + folder tree sorted by size
- Treemap
- File type breakdown
- Large files and old files lists
- Duplicate finder (size → partial hash → full hash)
- Junk cleaner (review list, safe items pre-checked)
- Browser cleaner: Chrome, Edge, Brave, Firefox. Cache is pre-checked. History and cookies are unchecked.
- Dev junk cleaner: project folders, global tool caches, Docker/WSL, IDE caches
- Installed apps manager: list, size, run the official uninstaller
- Scan cache + live updates from the NTFS USN change journal
- Search in scan results
- Export report (CSV, HTML)
- Tray icon + low space alert
- Explorer right-click menu "Analyze with Lustro"

---

## 2. Defaults that I chose (tell me to change any of them)

1. **UI library:** own CSS components, ported from the approved mockup (`src/styles/app.css`). The plan first said Fluent UI React v9. The mockup look was approved, so Lustro keeps it and does not load Fluent UI.
2. **License:** MIT.
3. **Installer:** NSIS, per-user install. This install does not need admin. The auto-update also does not need admin.
4. **Right-click menu:** The classic menu entry ("Show more options"). The Windows 11 top-level menu needs a signed MSIX package. Add it after code signing.
5. **"Last used" for apps:** Best effort only. Windows does not keep a reliable "last used" date for all apps.
6. **"Old files":** Use the last-modified date. Windows 11 does not always update the last-access date, so the last-access date is secondary.
7. **Windows.old:** Show it as an admin cleaner item, unchecked by default.
8. **Data location:** `%LOCALAPPDATA%\Lustro\` (cache, settings, delete history).
9. **Cleaner delete:** the cleaner deletes its items permanently, not to the Recycle Bin. They are caches and temp files that apps make again.
10. **Folder scans in the sidebar:** each scanned folder gets an entry under "Scanned folders".

---

## 3. Architecture

```
┌──────────────────────────── Lustro.exe (Tauri) ────────────────────────────┐
│  WebView2 (React + Fluent UI)                                              │
│    Home · Explore (tree + treemap) · Files · Duplicates · Clean · Apps ·   │
│    Search · History · Settings                                             │
│                    ▲ events (progress)      │ commands (typed IPC)         │
│                    │                        ▼                              │
│  Rust core                                                                 │
│    scan/      mft.rs (raw $MFT parse) · walk.rs (parallel FindFirstFileEx) │
│               usn.rs (change journal) · cache.rs (binary snapshot)         │
│    model/     node arena, string pool, size roll-up, type categories       │
│    ops/       delete.rs (Recycle Bin / permanent) · protect.rs · history   │
│    clean/     rules (data file) · browsers · dev · docker/wsl · ide        │
│    dupes/     size groups → partial BLAKE3 → full BLAKE3                   │
│    apps/      registry Uninstall keys · UWP PackageManager                 │
│    search/    name / wildcard / filters over the arena                     │
│    system/    elevation (ShellExecute "runas") · drives · tray · alerts    │
└────────────────────────────────────────────────────────────────────────────┘
```

### 3.1 Scan engine

**Fast scan (admin, NTFS):**
1. Open the volume `\\.\C:` with `CreateFileW`.
2. Read the volume data with `FSCTL_GET_NTFS_VOLUME_DATA`.
3. Read the data runs of `$MFT` from MFT record 0.
4. Read the `$MFT` in large blocks (for example 4 MB). Parse the FILE records in parallel with `rayon`.
5. From each record, read `$FILE_NAME` (name, parent) and `$DATA` (logical size, allocated size). Follow `$ATTRIBUTE_LIST` for large files.
6. Build the tree from the parent references. Count hard links one time only.

**Normal scan (no admin, or not NTFS):**
- Use a parallel walk with `FindFirstFileExW` + `FIND_FIRST_EX_LARGE_FETCH`.
- Show "access denied" folders in the tree with a lock icon and a hint: "Run as admin to see this folder".

**Rules for both scans:**
- Do not follow junctions or symbolic links. Show them, but do not count their target.
- Do not open OneDrive online-only files. Show their allocated size (zero) and their logical size.
- Send progress events to the UI (files counted, bytes, current path).
- Support cancel.

**Data model:** one `Vec<Node>` arena. Each node has a parent index, a first-child index, a next-sibling index, a name offset in a string pool, logical size, allocated size, modified time, attributes, and flags. Target: about 60 bytes per node, so 2 million files use about 120 MB.

**Cache and live updates:**
- After a scan, save a binary snapshot for each volume (with `rkyv` or `bincode`). Save the last USN number with it.
- At the next start, load the snapshot at once. Then read `FSCTL_READ_USN_JOURNAL` from the saved USN and apply only the changes.
- If the journal was reset or the gap is too large, do a full scan.
- USN updates need admin. Without admin, show the cached result with its date and a "Rescan" button.

### 3.2 Delete and protection

- **Recycle Bin:** `IFileOperation` with `FOFX_RECYCLEONDELETE`.
- **Permanent:** a Rust recursive delete with long path support (`\\?\` prefix). The delete never goes through a junction. It removes only the junction itself.
- **Protection:** the Rust backend checks each path before each delete. The UI check is only for display. Blocked paths:
  - `C:\Windows`, `C:\Program Files`, `C:\Program Files (x86)`, `C:\ProgramData\Microsoft`
  - `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`, `DumpStack.log`
  - `System Volume Information`, the `$Recycle.Bin` root, boot files, NTFS metafiles (`$MFT` and others)
  - Each drive root, each user profile root, and the `Users` folder
  - Files with the System attribute in the folders above
- **Exceptions:** cleaner rules can remove known safe junk inside protected folders, for example `C:\Windows\Temp` and `C:\Windows\SoftwareDistribution\Download`. The rule file lists these exceptions. Manual delete in the file tree is always blocked there.
- **History:** a JSON file (`history.json`, last 5000 entries): time, path, size, action (Recycle Bin, permanent, cleaner, blocked), result. The plan first said SQLite; a JSON file is enough and needs no C library.
- **Restore:** read the `$I` file in `$Recycle.Bin\<SID>\` to find the original path. Move the matching `$R` item back. If the item is no longer in the Recycle Bin, disable the button.

### 3.3 Cleaners

All cleaner rules are in a data file (TOML) in the app. Each rule has: id, category, paths/globs, "needs admin", "pre-checked", a description, and an optional command.

**System and user junk:** user temp, Windows temp (admin), Windows Update download cache (admin, stops and starts `wuauserv`), Delivery Optimization cache (admin), thumbnail cache, DirectX shader cache, crash dumps and WER reports, old log files, Recycle Bin (`SHEmptyRecycleBin`), Windows.old (admin, unchecked).

**Browsers:** find all profiles of Chrome, Edge, Brave, and Firefox.
- Cache: pre-checked.
- History and cookies: unchecked. The UI tells the user that cookies log them out of sites.
- If the browser runs, show "Close Chrome to clean it" and skip it.
- Never touch saved passwords, bookmarks, or extensions.

**Dev junk:**
- Project folders: find markers, for example `package.json` + `node_modules`, `Cargo.toml` + `target`, `*.csproj` + `bin`/`obj`, `__pycache__`, `.venv`, `build`/`dist`, `.gradle`. Pre-check a folder only when the project had no change for 30 days. You can change the number of days.
- Global caches: npm, yarn, pnpm, pip, cargo registry, NuGet, Gradle, Maven. Use the official command when one exists (for example `npm cache clean --force`, `pip cache purge`).
- Docker: show the image, container, and volume sizes. Run `docker system prune` after a confirm.
- WSL: show the size of each `ext4.vhdx`. Compact it with `wsl --shutdown` and then `diskpart` `compact vdisk` (admin).
- IDE: VS Code caches and old extension versions, JetBrains caches of old versions, Visual Studio component cache.

### 3.4 Duplicates

1. Use the scan result. Group files by exact size. Skip empty files, hard links of the same file, online-only files, and files below a minimum size (default 1 MB).
2. Hash the first and last 64 KB with BLAKE3. Keep only the groups that match.
3. Hash the full file. Keep only the groups that match.
4. UI: groups with the space you can save. Auto-select helpers: "keep newest", "keep oldest", "keep the one in folder X". At least one copy of each group must stay unselected.

### 3.5 Installed apps

- Win32 apps: read `HKLM` and `HKCU` `...\CurrentVersion\Uninstall` (64-bit and 32-bit views). Use `EstimatedSize`. If it is missing, use the install folder size from the scan.
- Store apps: `Windows.Management.Deployment.PackageManager` (WinRT through the `windows` crate).
- Uninstall: run the `UninstallString` of the app, or `RemovePackageAsync` for Store apps. Do not remove system Store apps.

### 3.6 Search, export, tray, right-click menu

- **Search:** parallel name match (contains, wildcard) over the arena with filters for size, date, and type. Results show in a virtual list.
- **Export:** CSV of any list. One HTML report file with the summary, top folders, and type breakdown.
- **Tray:** a small non-admin process. It checks free space each 15 minutes. It sends a Windows notification when a drive goes below the limit (default 10% free). An option starts it with Windows.
- **Right-click menu:** a `HKCU\Software\Classes\Directory\shell\Lustro` entry (and `Drive\shell`). It starts `Lustro.exe --scan "<path>"`.

### 3.7 Security

The app can run as admin and can delete files. Thus:
- Load only the bundled UI files. Use a strict CSP. Load no remote content.
- Give the UI only the Tauri commands that it needs (Tauri capabilities).
- Do all path checks in Rust. Do not trust paths from the UI.
- Do not let the UI run any shell command. Cleaner commands are fixed in the rule file.

---

## 4. Milestones

Each milestone ends with a commit and a push to `master`. Each milestone is also a GitHub milestone with the same name. The "Phase" column shows the phase from the first plan.

| # | Milestone | Phase |
|---|---|---|
| M0 | Design mockup | 0 |
| M1 | Project skeleton and CI | 1 |
| M2 | Scan engine | 1 |
| M3 | Explore UI | 1 |
| M4 | Delete and safety | 1 |
| M5 | System and browser cleaner | 2 |
| M6 | Dev, Docker/WSL and IDE cleaner | 2 |
| M7 | Duplicates and installed apps | 3 |
| M8 | Search and live updates | 3 |
| M9 | Tray, Explorer menu and export | 4 |
| M10 | Release v1.0.0 | 4 |

### M0 — Design mockup

- A clickable HTML mockup in `mockup/index.html`. Fluent style, light and dark.
- Screens: Home, Explore, Large and old files, Duplicates, Clean up, Installed apps, History, Search, Settings.
- **Exit:** you approve the look.

### M1 — Project skeleton and CI

- Tauri 2 + React + TypeScript + Vite project. Components from the mockup design. i18next with an English file. One placeholder logo (`src/assets/logo.svg`) for the window, the app icons and the installer.
- Window with Mica, title bar, sidebar navigation, theme (System / Light / Dark).
- `asInvoker` manifest. Elevation check and the "Run as admin" restart (`ShellExecuteW` with `runas`).
- Drive list with used and free space (`GetLogicalDrives`, `GetDiskFreeSpaceExW`, `GetVolumeInformationW`).
- GitHub Actions on `windows-latest`: `cargo fmt --check`, `cargo clippy`, `cargo test`, frontend lint and tests, NSIS installer build.
- **Exit:** the installer from CI installs and starts. The drive list is correct. "Run as admin" restarts the app elevated.

### M2 — Scan engine

- Node arena and string pool. Size roll-up. File type categories.
- Parallel folder walk (`FindFirstFileExW` + `FIND_FIRST_EX_LARGE_FETCH`). Junctions are not followed. "Access denied" folders are marked.
- MFT reader: volume handle, `$MFT` data runs, FILE record parse, `$ATTRIBUTE_LIST`, hard links. Automatic fallback to the walk.
- Progress events and cancel. Binary scan cache for each volume.
- **Exit:** the MFT scan and the walk give the same totals on C:. The total is within 1% of the used space that Windows shows. 1 million files scan in less than 15 s as admin.

### M3 — Explore UI

- Home screen: drive cards and the "Ways to free space" cards.
- Folder tree (virtual list), canvas treemap (squarified, nested, zoom, breadcrumb), file type view, detail pane.
- Large and old files lists with filters.
- **Exit:** the UI stays smooth with 2 million nodes. The screens match the approved mockup.

### M4 — Delete and safety

- Recycle Bin delete (`IFileOperation`) and permanent delete (long paths, no junction traversal).
- Protected path rules in the backend. Confirm dialogs.
- Delete history (JSON). Restore from the Recycle Bin through the Windows shell (`trash` crate).
- **Exit:** delete and restore work. Each protected path is blocked in tests. A junction delete never touches the target.

### M5 — System and browser cleaner

- TOML rule file and rule engine. "Needs admin" items. Review list with safe items pre-checked.
- System junk: temp folders, Windows Update cache, Delivery Optimization, thumbnail cache, shader cache, crash dumps, Recycle Bin, Windows.old.
- Browsers: Chrome, Edge, Brave, Firefox. Cache pre-checked. History and cookies unchecked. Skip a browser that runs.
- **Exit:** each rule shows the correct size. Cleaning frees the expected space. Nothing outside the rule paths changes.

### M6 — Dev, Docker/WSL and IDE cleaner

- Project folder finder (node_modules, target, bin/obj, .venv, __pycache__, build/dist, .gradle) with the 30-day rule.
- Global tool caches, deleted by path (the plan first said each tool's own clean command; deleting the cache folders gives the same result and works when the tool is not on PATH).
- Docker prune commands. WSL `ext4.vhdx` compact (admin).
- VS Code, JetBrains and Visual Studio caches.
- **Exit:** active projects stay unchecked. Each command runs and its output shows in the UI.

### M7 — Duplicates and installed apps

- Duplicate finder: size groups, partial BLAKE3, full BLAKE3. Keep rules. At least one copy stays.
- Installed apps: registry (both views, HKLM and HKCU) and Store packages. Size, install date, last used (best effort). Run the uninstaller.
- **Exit:** the finder finds all copies in a test folder and no wrong matches. Uninstall starts the correct uninstaller.

### M8 — Search and live updates

- Search over the arena: contains and wildcards, filters for size, date and type.
- USN change journal: apply changes to the arena and the cache. Full rescan when the journal is reset.
- **Exit:** a search over 2 million files takes less than 100 ms. A change in Explorer shows in Lustro without a rescan.

### M9 — Tray, Explorer menu and export

- Tray process with the low space alert. "Start with Windows" option.
- Classic right-click menu entry "Analyze with Lustro" for folders and drives.
- CSV export of lists. HTML report.
- **Exit:** the alert shows when free space goes below the limit. The menu entry opens the folder in Lustro.

### M10 — Release v1.0.0

- Tauri updater with its own signing key. `latest.json` on GitHub Releases.
- Release workflow on a version tag. README with screenshots. Translation check.
- **Exit:** install from GitHub, update from an older version, uninstall without leftovers.

---

## 5. Tests

- **Rust unit tests:** MFT record parser with fixed test bytes, protection rules, cleaner rule match, duplicate grouping, USN change apply.
- **Rust integration tests:** walker, delete, and restore on a temp folder tree with junctions, hard links, long paths, and Unicode names.
- **Manual check:** compare the MFT scan and the walker scan on the same drive. The totals must match.
- **Frontend:** Vitest for the treemap layout and size format functions.
- **CI:** GitHub Actions on `windows-latest`: `cargo test`, `cargo clippy`, frontend tests, and an installer build.

## 6. Performance targets

- Fast scan: 1 million files in less than 15 seconds on an SSD.
- Normal scan: as fast as the disk lets it, with the UI always responsive.
- Memory: less than 300 MB for 2 million files.
- UI: smooth scroll and treemap zoom at 60 fps.

## 7. Risks

| Risk | Plan |
|---|---|
| The MFT parser has bugs on unusual volumes | Compare with the walker. If the parse fails, use the walker automatically. |
| A wrong delete | Backend protection, Recycle Bin default, confirm dialogs, history. |
| SmartScreen warns about the unsigned .exe | Tell users in the README. Add code signing later. |
| Antivirus marks the app (raw disk read + delete) | Open source code. Submit false-positive reports. Code signing later. |
| No drag-and-drop from Explorer when admin | Windows blocks it. Use the folder picker and the right-click menu. |

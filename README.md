# Lustro

Lustro is a free and open source storage manager for Windows 11. It scans your drives, shows what uses the space, and helps you remove what you do not need.

*Lustro* is Latin. It means "I purify" and also "I survey".

> **Status:** early development. There is no release yet. The design mockup is in [`mockup/index.html`](mockup/index.html). Open the file in a browser. CI builds an installer for each push (see the Actions tab).

## Planned features

- **Fast scan.** With admin rights, Lustro reads the NTFS Master File Table directly. A full drive takes seconds. Without admin rights, it walks the folders.
- **Explore.** A folder tree sorted by size, a treemap, and a file type breakdown.
- **Large and old files.** Find the biggest files and the files that nobody changed for a long time.
- **Duplicates.** Find identical files by size and content hash.
- **Clean up.** Temp files, Windows Update cache, browser caches, crash dumps, and dev junk (node_modules, Rust target folders, tool caches, Docker images, WSL disks, IDE caches).
- **Installed apps.** See the size of each app and run its uninstaller.
- **Safe delete.** Recycle Bin by default, a delete history with restore, and protected system paths that Lustro never deletes.
- **Live updates, search, export, low space alert, and an Explorer right-click menu.**

The full plan is in [`PLAN.md`](PLAN.md).

## Tech

Tauri 2, Rust, React, TypeScript.

## Build from source

You need Windows 11, [Rust](https://rustup.rs) (MSVC toolchain), Node.js 22 or later, and the Visual Studio C++ build tools.

```powershell
npm install
npm run tauri dev     # run the app in dev mode
npm run tauri build   # make the installer in src-tauri/target/release/bundle/nsis
npm test              # frontend tests
cd src-tauri; cargo test
```

If the Rust build fails with `link.exe` errors, another `link.exe` (from Git, Cygwin or MSYS) is earlier in your `PATH` than the MSVC linker. Remove those folders from `PATH` in that terminal, or build from a "Developer PowerShell for VS".

## Logo

The logo is a placeholder. To change it, replace `src/assets/logo.svg` and run `npm run icons`. The app, the window and the installer all use this one file.

## License

[MIT](LICENSE)

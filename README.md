# Lustro

Lustro is a free and open source storage manager for Windows 11. It scans your drives, shows what uses the space, and helps you remove what you do not need.

*Lustro* is Latin. It means "I purify" and also "I survey".

> **Status:** early development. There is no release yet. The design mockup is in [`mockup/index.html`](mockup/index.html). Open the file in a browser.

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

Tauri 2, Rust, React, TypeScript, Fluent UI.

## License

[MIT](LICENSE)

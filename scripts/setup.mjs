// Builds or runs Lustro Setup (setup/), the themed installer.
//
//   node scripts/setup.mjs dev     run the setup window in dev mode (no installer inside)
//   node scripts/setup.mjs build   build Lustro-Setup-<version>.exe around the NSIS installer
//
// Build the app installer first (npm run tauri build). The result goes to
// src-tauri/target/release/bundle/setup/.

import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const setupDir = join(root, "setup");
const version = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8")).version;
const mode = process.argv[2];

function tauri(args, env = {}) {
  // One command string: npx is a .cmd file on Windows, so it needs a shell.
  const r = spawnSync(`npx tauri ${args.join(" ")}`, { cwd: setupDir, stdio: "inherit", shell: true, env: { ...process.env, ...env } });
  if (r.status !== 0) process.exit(r.status ?? 1);
}

if (mode === "dev") {
  tauri(["dev"]);
} else if (mode === "build") {
  const payload = process.env.LUSTRO_PAYLOAD ?? join(root, "src-tauri", "target", "release", "bundle", "nsis", `Lustro_${version}_x64-setup.exe`);
  if (!existsSync(payload)) {
    console.error(`No NSIS installer at ${payload}.\nRun "npm run tauri build" first, or set LUSTRO_PAYLOAD.`);
    process.exit(1);
  }
  tauri(["build", "--no-bundle"], { LUSTRO_PAYLOAD: payload });
  const outDir = join(root, "src-tauri", "target", "release", "bundle", "setup");
  mkdirSync(outDir, { recursive: true });
  const out = join(outDir, `Lustro-Setup-${version}.exe`);
  copyFileSync(join(setupDir, "src-tauri", "target", "release", "lustro-setup.exe"), out);
  console.log(`\nLustro Setup: ${out} (${(statSync(out).size / 1048576).toFixed(1)} MB)`);
} else {
  console.error("Usage: node scripts/setup.mjs dev|build");
  process.exit(1);
}

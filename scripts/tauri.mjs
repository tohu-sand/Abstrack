// Runs the Tauri CLI. For `build`, the home folder in the source paths that
// Rust writes into the program (where a dependency would report a panic)
// becomes "~", so that the program handed out does not carry the user name
// of the machine that built it. Cargo's trim-paths would do this, but it
// needs a nightly toolchain.
// Usage: node scripts/tauri.mjs <tauri arguments>   (npm run tauri ...)

import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { homedir } from "node:os";

const args = process.argv.slice(2);
const env = { ...process.env };
if (args[0] === "build") {
  // The encoded form keeps a home folder with spaces in one argument.
  const earlier = env.CARGO_ENCODED_RUSTFLAGS ?? (env.RUSTFLAGS ?? "").split(/\s+/).filter(Boolean).join("\x1f");
  env.CARGO_ENCODED_RUSTFLAGS = [earlier, `--remap-path-prefix=${homedir()}=~`].filter(Boolean).join("\x1f");
  delete env.RUSTFLAGS;
}
const cli = createRequire(import.meta.url).resolve("@tauri-apps/cli/tauri.js");
const result = spawnSync(process.execPath, [cli, ...args], { stdio: "inherit", env });
if (result.error) throw result.error;
process.exit(result.status ?? 1);

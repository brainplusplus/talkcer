// Launch the web dev server from a script, not a shell command line.
//
// `dev.mjs` spawns this with node and then exits. A child using inherited
// stdio is torn down when that parent exits, so this script detaches the dev
// server, records its pid next to the stack's pid file, and exits itself.
// Passing the npm CLI path as a spawn argument avoids the shell quoting that
// splits "Program Files" apart.

import { spawn } from "node:child_process";
import { appendFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const npmCli = resolve(dirname(process.execPath), "node_modules", "npm", "bin", "npm-cli.js");

const child = spawn(process.execPath, [npmCli, "run", "dev"], {
  cwd: resolve(root, "apps", "web"),
  detached: true,
  stdio: "ignore",
  windowsHide: true,
});
child.unref();

mkdirSync(resolve(root, "logs"), { recursive: true });
appendFileSync(resolve(root, "logs", "dev.pids"), `\n${child.pid}`);

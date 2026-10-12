// Start or stop the whole Talkcer dev stack from one npm script.
//
// `npm run dev` builds the API binary, starts it on port 54000, and starts the
// web dev server on port 54001. `npm run dev:stop` stops exactly the processes
// this script started, recorded in logs/dev.pids, so nothing else on the
// machine is touched. No extra tooling is required: this is plain Node.

import { execFileSync, spawn } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const pidFile = resolve(root, "logs", "dev.pids");

const command = process.argv[2];

if (command === "start") {
  start();
} else if (command === "stop") {
  stop();
} else {
  console.error("usage: node scripts/dev.mjs <start|stop>");
  process.exit(1);
}

function start() {
  mkdirSync(dirname(pidFile), { recursive: true });

  // Restarting must not leave the previous stack serving the same ports.
  stopRecorded();

  console.log("building talkcer-cli (first run compiles, later runs are cached)…");
  execFileSync("cargo", ["build", "-p", "talkcer-cli"], { cwd: root, stdio: "inherit" });

  const pids = [
    launch("api", resolve(root, "target/debug/talkcer-cli.exe"), ["serve", "--database", "talkcer.sqlite"]),
    launch("web", process.execPath, [resolve(root, "scripts", "web-dev.mjs")]),
  ];
  writeFileSync(pidFile, pids.join("\n"));

  console.log("api   http://127.0.0.1:54000");
  console.log("web   http://127.0.0.1:54001");
  console.log("stop  npm run dev:stop");
}

function launch(name, file, args, cwd = root) {
  const child = spawn(file, args, { cwd, detached: true, stdio: "ignore", windowsHide: true });
  child.unref();
  console.log(`started ${name} (pid ${child.pid})`);
  return child.pid;
}

function stop() {
  const stopped = stopRecorded();
  console.log(stopped.length === 0 ? "nothing running" : `stopped ${stopped.join(", ")}`);
}

// Stop the processes recorded at start, plus the descendants they spawned
// (the web entry spawns vite). Anything not in the record is left running.
function stopRecorded() {
  const recorded = readPids();
  if (recorded.length === 0) return [];
  const children = childIndex();
  const targets = recorded
    .map((pid) => subtree(pid, children))
    .filter((tree) => !tree.includes(process.pid))
    .flat()
    // Leaves first, so killing a parent cannot reparent its children out of reach.
    .reverse();
  for (const pid of targets) {
    try {
      // One failure must not skip the rest: a recorded pid may already have
      // exited, and Windows treats stopping it as an error.
      powershell(`Stop-Process -Id ${pid} -Force -ErrorAction SilentlyContinue`);
    } catch {
      // The process exited between the lookup and the stop.
    }
  }
  writeFileSync(pidFile, "");
  return targets;
}

function readPids() {
  try {
    return readFileSync(pidFile, "utf8")
      .split("\n")
      .map((line) => Number.parseInt(line.trim(), 10))
      .filter((pid) => Number.isInteger(pid));
  } catch {
    return [];
  }
}

// Map each running process id to the ids it spawned.
function childIndex() {
  const rows = parseProcesses(
    powershell(
      "Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId | ConvertTo-Json -Compress",
    ),
  );
  const children = new Map();
  for (const row of rows) {
    const kids = children.get(row.ParentProcessId) ?? [];
    kids.push(row.ProcessId);
    children.set(row.ParentProcessId, kids);
  }
  return children;
}

// One recorded pid plus the processes it spawned, depth first.
function subtree(root, children) {
  const found = [];
  const stack = [root];
  while (stack.length > 0) {
    const pid = stack.pop();
    found.push(pid);
    for (const child of children.get(pid) ?? []) stack.push(child);
  }
  return found;
}

function parseProcesses(listing) {
  const trimmed = listing.trim();
  if (trimmed.length === 0) return [];
  const parsed = JSON.parse(trimmed);
  return Array.isArray(parsed) ? parsed : [parsed];
}

function powershell(script) {
  return execFileSync("powershell.exe", ["-NoProfile", "-Command", script], { encoding: "utf8" });
}

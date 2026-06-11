import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appDir = resolve(scriptDir, "..");
const repoRoot = resolve(appDir, "../..");
const serverManifest = resolve(repoRoot, "apps/echo-http-server/Cargo.toml");
const distDir = resolve(appDir, "dist");

await runTogether(
  {
    command: "npm",
    args: ["exec", "--", "vite", "build", "--watch"],
    options: { cwd: appDir },
  },
  {
    command: "cargo",
    args: ["run", "--manifest-path", serverManifest],
    options: {
      cwd: repoRoot,
      env: {
        ...process.env,
        ECHO_WEB_DIST: distDir,
        ECHO_WEB_PORT: process.env.ECHO_WEB_PORT || httpPort(process.env.ECHO_HTTP_BIND),
      },
    },
  },
);

function runTogether(...processes) {
  return new Promise((resolvePromise, reject) => {
    const children = processes.map(({ command, args, options }) =>
      spawn(command, args, {
        ...options,
        stdio: "inherit",
        shell: process.platform === "win32",
      }),
    );

    let settled = false;

    function stopOthers(exitingChild) {
      for (const child of children) {
        if (child !== exitingChild && child.exitCode === null && child.signalCode === null) {
          stopProcessTree(child);
        }
      }
    }

    function stopAll() {
      for (const child of children) {
        if (child.exitCode === null && child.signalCode === null) {
          stopProcessTree(child);
        }
      }
    }

    for (const child of children) {
      child.on("exit", (code, signal) => {
        if (settled) return;
        settled = true;
        stopOthers(child);

        if (code === 0) {
          resolvePromise();
        } else {
          reject(new Error(`${child.spawnfile} exited with ${signal ?? code}`));
        }
      });
      child.on("error", (error) => {
        if (settled) return;
        settled = true;
        stopOthers(child);
        reject(error);
      });
    }

    process.once("SIGINT", () => {
      stopAll();
      process.exit(130);
    });
    process.once("SIGTERM", () => {
      stopAll();
      process.exit(143);
    });
  });
}

function stopProcessTree(child) {
  if (process.platform === "win32") {
    spawnSync("taskkill", ["/pid", String(child.pid), "/t", "/f"], {
      stdio: "ignore",
      shell: false,
    });
  } else {
    child.kill();
  }
}

function httpPort(bind) {
  if (!bind) return "4399";
  const marker = bind.lastIndexOf(":");
  return marker >= 0 ? bind.slice(marker + 1) : "4399";
}

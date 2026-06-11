import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appDir = resolve(scriptDir, "..");
const repoRoot = resolve(appDir, "../..");
const serverManifest = resolve(repoRoot, "apps/echo-http-server/Cargo.toml");
const distDir = resolve(appDir, "dist");

await run("npm", ["run", "build"], { cwd: appDir });

await run(
  "cargo",
  ["run", "--manifest-path", serverManifest],
  {
    cwd: repoRoot,
    env: {
      ...process.env,
      ECHO_WEB_DIST: distDir,
      ECHO_WEB_PORT: process.env.ECHO_WEB_PORT || httpPort(process.env.ECHO_HTTP_BIND),
    },
  },
);

function run(command, args, options) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, {
      ...options,
      stdio: "inherit",
      shell: process.platform === "win32",
    });

    child.on("exit", (code, signal) => {
      if (code === 0) {
        resolvePromise();
      } else {
        reject(new Error(`${command} exited with ${signal ?? code}`));
      }
    });
    child.on("error", reject);
  });
}

function httpPort(bind) {
  if (!bind) return "4399";
  const marker = bind.lastIndexOf(":");
  return marker >= 0 ? bind.slice(marker + 1) : "4399";
}

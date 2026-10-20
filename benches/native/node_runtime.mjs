/** Find npm's JavaScript entry without assuming a Windows Node installation. */
import fs from "node:fs";
import path from "node:path";

const cliSuffix = ["node_modules", "npm", "bin", "npm-cli.js"];

function realFile(file) {
  try {
    const resolved = fs.realpathSync(file);
    return fs.statSync(resolved).isFile() ? resolved : undefined;
  } catch {
    return undefined;
  }
}

function beside(directory) {
  return [
    path.join(directory, ...cliSuffix),
    path.resolve(directory, "..", "lib", ...cliSuffix),
  ];
}

/**
 * Prefer the npm bundled with the running Node. A PATH fallback handles distro
 * installations (for example /usr/bin/npm -> /usr/share/nodejs/npm/bin/npm-cli.js)
 * and Windows npm.cmd shims; the returned entry is always a real JS file.
 * Optional paths let contract tests build directories, without running fake npm.
 */
export function resolveNpmCli({ execPath = process.execPath, env = process.env } = {}) {
  const logicalNode = path.resolve(execPath);
  const realNode = realFile(logicalNode);
  const candidates = [];
  for (const node of new Set([realNode, logicalNode].filter(Boolean))) {
    candidates.push(...beside(path.dirname(node)));
  }
  for (const candidate of candidates) {
    const file = realFile(candidate);
    if (file && path.basename(file) === "npm-cli.js") return file;
  }
  const pathValue = Object.entries(env).find(([key]) => key.toLowerCase() === "path")?.[1] ?? "";
  for (let directory of pathValue.split(path.delimiter).filter(Boolean)) {
    directory = directory.replace(/^"(.*)"$/, "$1");
    for (const name of ["npm", "npm.cmd", "npm.exe"]) {
      const wrapper = realFile(path.resolve(directory, name));
      if (!wrapper) continue;
      if (path.basename(wrapper) === "npm-cli.js") return wrapper;
      for (const candidate of [...beside(path.dirname(wrapper)), ...beside(path.resolve(directory))]) {
        const file = realFile(candidate);
        if (file && path.basename(file) === "npm-cli.js") return file;
      }
    }
  }
  throw new Error("Cannot locate npm-cli.js beside Node or npm on PATH; install Node.js with npm.");
}

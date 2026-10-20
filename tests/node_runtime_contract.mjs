/** npm path layout contracts. Fake fixtures are never executed as npm/editors. */
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { execFileSync } from "node:child_process";
import { resolveNpmCli } from "../benches/native/node_runtime.mjs";

function file(base, relative) {
  const full = path.join(base, relative);
  fs.mkdirSync(path.dirname(full), { recursive: true });
  fs.writeFileSync(full, "// locator fixture, never executed\n");
  return full;
}
function fixture(fn) {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), "xnip-npm-locator-"));
  try { return fn(base); }
  finally { fs.rmSync(base, { recursive: true, force: true }); }
}

test("the real running Node installation resolves and runs real npm --version", () => {
  const cli = resolveNpmCli();
  assert.equal(path.basename(cli), "npm-cli.js");
  assert.ok(path.isAbsolute(cli));
  assert.match(execFileSync(process.execPath, [cli, "--version"], { encoding: "utf8", timeout: 15000 }).trim(),
    /^\d+\.\d+\.\d+(?:[-+].*)?$/);
});

test("Windows node.exe adjacent node_modules layout", () => fixture(base => {
  const node = file(base, "windows/node.exe");
  const npm = file(base, "windows/node_modules/npm/bin/npm-cli.js");
  assert.equal(resolveNpmCli({ execPath: node, env: {} }), fs.realpathSync(npm));
}));

test("Linux version-manager prefix uses bin/node plus lib/node_modules", () => fixture(base => {
  const node = file(base, "linux/versions/node/v24/bin/node");
  const npm = file(base, "linux/versions/node/v24/lib/node_modules/npm/bin/npm-cli.js");
  assert.equal(resolveNpmCli({ execPath: node, env: {} }), fs.realpathSync(npm));
}));

test("macOS Cellar prefix uses bin/node plus lib/node_modules", () => fixture(base => {
  const node = file(base, "mac/Cellar/node/24.13.0/bin/node");
  const npm = file(base, "mac/Cellar/node/24.13.0/lib/node_modules/npm/bin/npm-cli.js");
  assert.equal(resolveNpmCli({ execPath: node, env: {} }), fs.realpathSync(npm));
}));

test("bundled npm wins over a different PATH npm", () => fixture(base => {
  const node = file(base, "node/bin/node");
  const bundled = file(base, "node/lib/node_modules/npm/bin/npm-cli.js");
  file(base, "global/npm.cmd");
  file(base, "global/node_modules/npm/bin/npm-cli.js");
  assert.equal(resolveNpmCli({ execPath: node, env: { PATH: path.join(base, "global") } }),
    fs.realpathSync(bundled));
}));

test("PATH Windows shim with spaces and case-insensitive Path resolves JS, never cmd", () => fixture(base => {
  const node = file(base, "node-only/node.exe");
  file(base, "global npm/npm.cmd");
  const npm = file(base, "global npm/node_modules/npm/bin/npm-cli.js");
  assert.equal(resolveNpmCli({ execPath: node, env: { Path: '"' + path.join(base, "global npm") + '"' } }),
    fs.realpathSync(npm));
}));

test("PATH npm realpath uses actual POSIX npm symlink when available", () => {
  // Real installed npm on POSIX is normally a symlink. The real-current test
  // also covers Windows, whose npm.cmd adjacent layout has a separate fixture.
  if (process.platform === "win32") {
    const npm = resolveNpmCli();
    const root = path.resolve(path.dirname(npm), "../../..");
    assert.equal(resolveNpmCli({ execPath: path.join(os.tmpdir(), "absent-node-install/node.exe"),
      env: { PATH: root } }), fs.realpathSync(npm));
    return;
  }
  fixture(base => {
    const node = file(base, "standalone/bin/node");
    const npm = file(base, "usr/share/nodejs/npm/bin/npm-cli.js");
    const wrapper = path.join(base, "usr/bin/npm");
    fs.mkdirSync(path.dirname(wrapper), { recursive: true });
    fs.symlinkSync(npm, wrapper);
    assert.equal(resolveNpmCli({ execPath: node, env: { PATH: path.dirname(wrapper) } }),
      fs.realpathSync(npm));
  });
});

test("missing or directory-shaped npm CLI fails without shell lookup", () => fixture(base => {
  const node = file(base, "node-only/node");
  fs.mkdirSync(path.join(base, "node-only/node_modules/npm/bin/npm-cli.js"), { recursive: true });
  assert.throws(() => resolveNpmCli({ execPath: node, env: {} }), /Cannot locate npm-cli\.js/);
}));

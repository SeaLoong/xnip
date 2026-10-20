#!/usr/bin/env node
"use strict";

const { spawn } = require("node:child_process");
const { resolveNativeExecutable, childEnvironment } = require("../lib/native.cjs");

let child;
try {
  child = spawn(resolveNativeExecutable(), process.argv.slice(2), {
    stdio: ["pipe", "pipe", "pipe"], shell: false, windowsHide: true, env: childEnvironment(),
  });
} catch (error) {
  process.stderr.write(error.message + "\n");
  process.exitCode = 1;
}
if (child) {
  // The wrapper owns the child stdin writer. Even an abrupt wrapper exit closes
  // that pipe, so an MCP server observes EOF instead of retaining the host pipe.
  process.stdin.pipe(child.stdin);
  child.stdout.pipe(process.stdout);
  child.stderr.pipe(process.stderr);
  child.stdin.on("error", error => {
    if (error.code !== "EPIPE" && error.code !== "ERR_STREAM_DESTROYED") {
      process.stderr.write("xnip: input forwarding failed: " + error.message + "\n");
    }
  });
  const listeners = new Map();
  for (const signal of ["SIGINT", "SIGTERM"]) {
    const listener = () => { if (child.exitCode === null) child.kill(signal); };
    listeners.set(signal, listener);
    process.on(signal, listener);
  }
  child.once("error", error => {
    process.stderr.write("xnip: cannot start executable: " + error.message + "\n");
    process.exitCode = 1;
  });
  child.once("close", (code, signal) => {
    for (const [name, listener] of listeners) process.removeListener(name, listener);
    process.stdin.unpipe(child.stdin);
    process.stdin.pause();
    if (!process.stdin.isTTY) process.stdin.destroy();
    process.exitCode = code !== null && code >= 0 ? code : (signal === "SIGINT" ? 130 : signal === "SIGTERM" ? 143 : 1);
  });
}

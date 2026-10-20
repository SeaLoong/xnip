/**
 * Register xnip through Pi's MCP lifecycle. Built-in edit/write remain unchanged.
 * The npm package copies this directory verbatim; do not maintain another entry.
 */
import { createRequire } from "node:module";
import path from "node:path";

const require = createRequire(import.meta.url);

function nativeExecutable() {
  const { resolveNativeExecutable } = require("xnip/native");
  return resolveNativeExecutable();
}

/** Dependency injection is for source tests; npm uses its immutable native cache. */
export function createXnipExtension({ resolveExecutable = nativeExecutable } = {}) {
  return function xnipExtension(pi) {
    if (typeof pi.registerMcpServer !== "function" || typeof pi.unregisterMcpServer !== "function") {
      throw new Error("xnip requires Pi's registerMcpServer/unregisterMcpServer extension API (verified with Pi 1.3.0).");
    }
    let registered = false;
    pi.on("session_start", (_event, ctx) => {
      const executable = resolveExecutable();
      if (typeof executable !== "string" || !path.isAbsolute(executable)) {
        throw new Error("xnip native resolver must return an absolute executable path.");
      }
      pi.registerMcpServer("xnip", {
        command: executable,
        args: ["mcp"],
        cwd: ctx.cwd,
        env: { XNIP_AUTO_UPDATE: "0" },
        exposure: "direct",
      });
      registered = true;
    });
    pi.on("session_shutdown", () => {
      if (registered) {
        pi.unregisterMcpServer("xnip");
        registered = false;
      }
    });
  };
}

export default createXnipExtension();

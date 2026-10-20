/** Explicit Cordis registration of xnip through DSH's official MCP client. */
import { createRequire } from 'node:module';
import { isAbsolute, resolve } from 'node:path';

const require = createRequire(import.meta.url);
const CONFIG_FIELDS = new Set(['cwd', 'env', 'serverName', 'receipts', 'toolset']);

function resolvePackagedExecutable() {
  const native = require('xnip/native');
  if (typeof native.resolveNativeExecutable !== 'function') {
    throw new TypeError('xnip/native must export resolveNativeExecutable()');
  }
  return native.resolveNativeExecutable();
}

async function loadOfficialBridge() {
  return import('@deepseek-ai/dsh-mcp-client');
}

/** Build the official stdio bridge configuration without starting a process. */
export function createMcpConfig(config = {}, resolveNativeExecutable = resolvePackagedExecutable) {
  if (config === null || typeof config !== 'object' || Array.isArray(config)) {
    throw new TypeError('xnip DSH configuration must be an object');
  }
  for (const field of Object.keys(config)) {
    if (!CONFIG_FIELDS.has(field)) throw new TypeError(`Unknown xnip DSH option: ${field}`);
  }
  const receipts = config.receipts ?? 'text';
  const toolset = config.toolset ?? 'compact';
  if (!['text', 'structured'].includes(receipts)) throw new TypeError('receipts must be text or structured');
  if (!['compact', 'full'].includes(toolset)) throw new TypeError('toolset must be compact or full');
  if (config.cwd !== undefined && (typeof config.cwd !== 'string' || config.cwd.trim() === '')) {
    throw new TypeError('cwd must be a nonempty directory path');
  }
  const serverName = config.serverName ?? 'xnip';
  if (typeof serverName !== 'string' || serverName.trim() === '') {
    throw new TypeError('serverName must be a nonempty string');
  }
  const env = config.env ?? {};
  if (typeof env !== 'object' || Array.isArray(env)) throw new TypeError('env must be a string map');
  for (const [key, value] of Object.entries(env)) {
    if (typeof value !== 'string') throw new TypeError(`env.${key} must be a string`);
  }
  const command = resolveNativeExecutable();
  if (typeof command !== 'string' || !isAbsolute(command)) {
    throw new TypeError('resolveNativeExecutable() must return an absolute executable path');
  }
  return {
    transport: 'stdio',
    serverName,
    command,
    args: ['mcp', '--receipts', receipts, ...(toolset === 'full' ? ['--toolset', 'full'] : [])],
    cwd: resolve(config.cwd ?? process.cwd()),
    env: { ...env, XNIP_AUTO_UPDATE: '0' },
    failOnStartupError: true,
  };
}

/** Dependencies belong to the host/test seam, never the saved user configuration. */
export function createXnipPlugin({
  resolveNativeExecutable = resolvePackagedExecutable,
  loadMcpClient = loadOfficialBridge,
} = {}) {
  if (typeof resolveNativeExecutable !== 'function' || typeof loadMcpClient !== 'function') {
    throw new TypeError('xnip DSH dependencies must be functions');
  }
  return {
    name: 'xnip-mcp',
    inject: ['tools'],
    async apply(ctx, config = {}) {
      if (typeof ctx?.plugin !== 'function') throw new TypeError('xnip DSH requires a Cordis context');
      const bridgeConfig = createMcpConfig(config, resolveNativeExecutable);
      const namespace = await loadMcpClient();
      const bridge = typeof namespace?.apply === 'function' ? namespace : namespace?.default;
      if (typeof bridge?.apply !== 'function') {
        throw new TypeError('The official DSH MCP client must export a Cordis apply function');
      }
      await ctx.plugin(bridge, bridgeConfig);
    },
  };
}

const plugin = createXnipPlugin();
export const name = plugin.name;
export const inject = plugin.inject;
export const apply = plugin.apply;
export default plugin;

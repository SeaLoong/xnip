/** Real OpenCode CLI plugin loading, registry and debug tool execution. */
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import http from 'node:http';
import path from 'node:path';
import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';
import { pathToFileURL } from 'node:url';
import {
  HOST,
  VERSION,
  executablePath,
  verifyOpenCode,
} from '../benches/native/opencode_native_setup.mjs';

const exec = promisify(execFile);
const root = path.resolve(import.meta.dirname, '..');
let verified;
export { VERSION };
export async function createFixture({
  cwd,
  executable = process.env.XNIP_TEST_BINARY ?? path.join(root, 'target/debug/xnip'),
  native = true,
  permission = {},
  mode,
  entry,
  childEnvironment = {},
  hostDirectory = HOST,
} = {}) {
  verified ??= verifyOpenCode(hostDirectory);
  const provenance = await verified;
  const state = await fs.mkdtemp(path.join(root, 'target/opencode-native-session-'));
  cwd ??= path.join(state, 'project');
  await fs.mkdir(cwd, { recursive: true });
  const configDir = path.join(state, 'config');
  await fs.mkdir(configDir);
  for (const name of ['package.json', 'package-lock.json'])
    await fs.copyFile(path.join(hostDirectory, name), path.join(configDir, name));
  await fs.symlink(
    path.join(hostDirectory, 'node_modules'),
    path.join(configDir, 'node_modules'),
    'dir',
  );
  const trace = path.join(state, 'trace.jsonl');
  const plugin = path.join(state, 'xnip.mjs');
  if (!entry)
    await fs.writeFile(
      plugin,
      `import fs from 'node:fs';
import {tool} from ${JSON.stringify(pathToFileURL(path.join(hostDirectory, 'node_modules/@opencode-ai/plugin/dist/tool.js')).href)};
import {createXnipPlugin} from ${JSON.stringify(pathToFileURL(path.join(root, 'integrations/opencode/plugin.mjs')).href)};
import {EditorClient} from ${JSON.stringify(pathToFileURL(path.join(root, 'integrations/native/editor-client.mjs')).href)};
const log=value=>fs.appendFileSync(${JSON.stringify(trace)},JSON.stringify(value)+'\\n');
export default async input=>{
 const hooks=await createXnipPlugin({loadTool:async()=>({tool}),resolveExecutable:()=>${JSON.stringify(executable)},createClient:(binary,cwd)=>{
  const client=new EditorClient(binary,cwd);log({event:'spawn',pid:client.child.pid});client.child.on('exit',()=>log({event:'exit',pid:client.child.pid}));
  const request=client.request.bind(client);client.request=async(action,fields)=>{const result=await request(action,fields);log({event:'request',action});
    if(action==='plan'&&${JSON.stringify(mode ?? '')}==='drift')fs.writeFileSync(result.changes[0].path,'external\\r\\n');
    return result;};return client;
 }})(input);
 return {...hooks,async dispose(){await hooks.dispose();log({event:'disposed'});}};
};
`,
    );
  let modelCalls = 0;
  const server = http.createServer((req, res) => {
    modelCalls++;
    res.writeHead(500);
    res.end('Tool-only fixture performed model request');
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const config = {
    model: 'xnip-fixture/tool-only',
    enabled_providers: ['xnip-fixture'],
    provider: {
      'xnip-fixture': {
        name: 'Tool-only',
        npm: '@ai-sdk/openai-compatible',
        options: { baseURL: `http://127.0.0.1:${server.address().port}/v1`, apiKey: 'tool-only' },
        models: { 'tool-only': { name: 'Tool-only', limit: { context: 128000, output: 8192 } } },
      },
    },
    plugin: native ? [entry ?? pathToFileURL(plugin).href] : [],
    permission: { '*': 'allow', ...permission },
    formatter: false,
    lsp: false,
  };
  const configPath = path.join(configDir, 'opencode.json');
  await fs.writeFile(configPath, JSON.stringify(config));
  const environment = {
    ...process.env,
    ...childEnvironment,
    XNIP_AUTO_UPDATE: '0',
    OPENCODE_CONFIG_DIR: configDir,
    OPENCODE_CONFIG: configPath,
    OPENCODE_DISABLE_PROJECT_CONFIG: 'true',
    OPENCODE_DISABLE_MODELS_FETCH: 'true',
    XDG_CONFIG_HOME: path.join(state, 'xdg/config'),
    XDG_DATA_HOME: path.join(state, 'xdg/data'),
    XDG_CACHE_HOME: path.join(state, 'xdg/cache'),
    XDG_STATE_HOME: path.join(state, 'xdg/state'),
  };
  let nextInvocation = 0;
  async function invoke(args) {
    // OpenCode's debug command exits immediately after stdout.write(). A regular
    // file descriptor keeps large JSON receipts complete across that CLI exit.
    const outputPath = path.join(state, `stdout-${++nextInvocation}.json`);
    const output = await fs.open(outputPath, 'w');
    let stderr = '';
    let code;
    try {
      code = await new Promise((resolve, reject) => {
        const child = spawn(executablePath(hostDirectory), ['debug', 'agent', 'build', ...args], {
          cwd,
          env: environment,
          stdio: ['ignore', output.fd, 'pipe'],
        });
        let timeout;
        const timer = setTimeout(() => {
          timeout = new Error('OpenCode CLI tool execution timed out after 60 seconds');
          child.kill('SIGTERM');
        }, 60000);
        child.stderr.setEncoding('utf8');
        child.stderr.on('data', (text) => {
          stderr += text;
        });
        child.once('error', (error) => {
          clearTimeout(timer);
          reject(error);
        });
        child.once('close', (status, signal) => {
          clearTimeout(timer);
          if (timeout) reject(timeout);
          else if (signal) reject(new Error(`OpenCode CLI exited from signal ${signal}`));
          else resolve(status);
        });
      });
    } finally {
      await output.close();
    }
    const stdout = await fs.readFile(outputPath, 'utf8');
    await fs.unlink(outputPath);
    assert.equal(modelCalls, 0, 'actual provider endpoint remains idle');
    const transport = {
      stdout_bytes: Buffer.byteLength(stdout),
      stderr_bytes: Buffer.byteLength(stderr),
    };
    if (code !== 0) return { ok: false, error: stderr || stdout, code, ...transport };
    let value;
    try {
      value = JSON.parse(stdout);
    } catch (cause) {
      throw new Error(
        `OpenCode CLI returned invalid JSON (${transport.stdout_bytes} stdout bytes)`,
        { cause },
      );
    }
    return { ok: true, value, stderr, ...transport };
  }
  return {
    cwd,
    state,
    trace,
    provenance,
    get modelCalls() {
      return modelCalls;
    },
    async describe() {
      const result = await invoke([]);
      assert.equal(result.ok, true, result.error);
      return result.value;
    },
    call(name, args) {
      return invoke(['--tool', name, '--params', JSON.stringify(args)]);
    },
    async events() {
      try {
        return (await fs.readFile(trace, 'utf8'))
          .trim()
          .split('\n')
          .filter(Boolean)
          .map((line) => JSON.parse(line));
      } catch (error) {
        if (error.code === 'ENOENT') return [];
        throw error;
      }
    },
    async management(args) {
      const result = await exec(executablePath(hostDirectory), args, {
        cwd,
        env: environment,
        encoding: 'utf8',
        timeout: 60000,
        maxBuffer: 4 * 1024 * 1024,
      });
      assert.equal(modelCalls, 0, 'actual provider endpoint remains idle');
      return result;
    },
    async setMcp(enabled) {
      config.mcp = enabled
        ? {
            xnip: {
              type: 'local',
              command: [executable, 'mcp', '--receipts', 'text'],
              enabled: true,
              environment: { XNIP_AUTO_UPDATE: '0' },
            },
          }
        : {};
      await fs.writeFile(configPath, JSON.stringify(config));
    },
    async setNative(enabled) {
      config.plugin = enabled ? [entry ?? pathToFileURL(plugin).href] : [];
      await fs.writeFile(configPath, JSON.stringify(config));
    },
    async close() {
      await new Promise((resolve) => server.close(resolve));
      assert.equal(modelCalls, 0);
      await fs.rm(state, { recursive: true, force: true });
    },
  };
}

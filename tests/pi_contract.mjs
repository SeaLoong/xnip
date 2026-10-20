import test, { before, afterEach } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { handle, COMMIT } from "../benches/native/pi_adapter.mjs";
import { createXnipExtension } from "../integrations/pi/index.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const executable = process.env.XNIP_PI_TEST_EXECUTABLE ?? path.join(ROOT, "target/debug", process.platform === "win32" ? "xnip.exe" : "xnip");
const temporaryBase = path.resolve(os.tmpdir());
const roots = [];
let description;
before(async () => {
  description = await handle({ action: "describe" });
  assert.equal(description.ok, true, description.error);
});
afterEach(async () => {
  await handle({ action: "reset" });
  for (const root of roots.splice(0)) {
    assert.ok(root.startsWith(path.join(temporaryBase, "xnip-pi-contract-")));
    await fs.rm(root, { recursive: true, force: true });
  }
});
async function fixture(content = "one=1\ntwo=2\n") {
  const root = await fs.mkdtemp(path.join(temporaryBase, "xnip-pi-contract-"));
  roots.push(root);
  await fs.writeFile(path.join(root, "a"), content);
  return root;
}
const edit = (root, edits, extra = {}) => handle({
  action: "execute", root, tool: "edit", arguments: { path: "a", edits }, ...extra,
});
const bytes = root => fs.readFile(path.join(root, "a"));

test("native pin, exact schemas and source hash evidence are exposed", () => {
  assert.equal(description.commit, COMMIT);
  assert.equal(description.model_calls, 0);
  assert.deepEqual(description.tools.edit.schema.required, ["path", "edits"]);
  assert.ok(description.tools.edit.schema.properties.edits);
  assert.equal(description.tools.edit.schema.properties.oldText, undefined);
  for (const ending of ["/edit.ts", "/edit-diff.ts", "/file-mutation-queue.ts", "/write.ts", "/validation.ts"]) {
    const source = description.source_revisions.find(x => x.file.endsWith(ending));
    assert.ok(source, ending);
    assert.match(source.sha256, /^[a-f0-9]{64}$/);
    assert.equal(source.unchanged, true);
  }
  assert.match(description.dependency_lock_sha256, /^[a-f0-9]{64}$/);
});

test("real multi-edit uses original coordinates and preserves BOM, CRLF and missing EOF LF", async () => {
  const root = await fixture("\ufeffone=1\r\ntwo=2");
  const result = await edit(root, [
    { oldText: "one=1", newText: "one=10000" },
    { oldText: "two=2", newText: "two=20" },
  ]);
  assert.equal(result.ok, true, result.error);
  assert.deepEqual(await bytes(root), Buffer.from("\ufeffone=10000\r\ntwo=20"));
  assert.match(result.output, /2 block/);
  assert.ok(result.native_result.details.patch.includes("-one=1"));
  assert.ok(!result.output.includes("-one=1"));
});

test("dependent edits, overlap, ambiguity, missing and no-op fail unchanged", async () => {
  for (const [source, edits] of [
    ["a", [{ oldText: "a", newText: "b" }, { oldText: "b", newText: "c" }]],
    ["abcde", [{ oldText: "abc", newText: "X" }, { oldText: "bcd", newText: "Y" }]],
    ["same same", [{ oldText: "same", newText: "new" }]],
    ["old", [{ oldText: "missing", newText: "new" }]],
    ["old", [{ oldText: "old", newText: "old" }]],
    ["old", [{ oldText: "", newText: "new" }]],
  ]) {
    const root = await fixture(source);
    const result = await edit(root, edits);
    assert.equal(result.ok, false, JSON.stringify(edits));
    assert.ok(result.error);
    assert.deepEqual(await bytes(root), Buffer.from(source));
  }
});

test("native fuzzy fallback and mixed EOL are retained rather than emulated away", async () => {
  const root = await fixture("keep  \nconst title = “old”;\nend  \n");
  const fuzzy = await edit(root, [{ oldText: 'const title = "old";', newText: 'const title = "new";' }]);
  assert.equal(fuzzy.ok, true, fuzzy.error);
  assert.deepEqual(await bytes(root), Buffer.from('keep  \nconst title = "new";\nend  \n'));
  await fs.writeFile(path.join(root, "a"), "one=1\r\ntwo=2\nthree=3\r\n");
  const mixed = await edit(root, [{ oldText: "two=2", newText: "two=20" }]);
  assert.equal(mixed.ok, true, mixed.error);
  assert.deepEqual(await bytes(root), Buffer.from("one=1\r\ntwo=20\r\nthree=3\r\n"));
});

test("overlapping literal occurrences keep Pi's native non-overlapping count", async () => {
  const root = await fixture("ababa");
  const result = await edit(root, [{ oldText: "aba", newText: "X" }]);
  assert.equal(result.ok, true, result.error);
  assert.deepEqual(await bytes(root), Buffer.from("Xba"));
});

test("same-process real mutation queue serializes concurrent edits", async () => {
  const root = await fixture("start\n");
  const [first, second] = await Promise.all([
    edit(root, [{ oldText: "start", newText: "middle" }]),
    edit(root, [{ oldText: "middle", newText: "done" }]),
  ]);
  assert.equal(first.ok, true, first.error);
  assert.equal(second.ok, true, second.error);
  assert.deepEqual(await bytes(root), Buffer.from("done\n"));
});

test("Pi argument preparation is real and abort does not write", async () => {
  const root = await fixture("old\n");
  const prepared = await edit(root, JSON.stringify([{ oldText: "old", newText: "new" }]));
  assert.equal(prepared.ok, true, prepared.error);
  const aborted = await edit(root, [{ oldText: "new", newText: "later" }], { aborted: true });
  assert.equal(aborted.ok, false);
  assert.match(aborted.error, /aborted/);
  assert.deepEqual(await bytes(root), Buffer.from("new\n"));
  const invalid = await handle({ action: "execute", root, tool: "edit", arguments: { path: "a", edits: [{ oldText: 1, newText: "bad" }] } });
  assert.equal(invalid.ok, false);
  assert.deepEqual(await bytes(root), Buffer.from("new\n"));
});

test("write is a real whole-file create/overwrite, including empty content", async () => {
  const root = await fixture();
  const created = await handle({ action: "execute", root, tool: "write", arguments: { path: "new/deep/a", content: "中\r\nno-eof" } });
  assert.equal(created.ok, true, created.error);
  assert.deepEqual(await fs.readFile(path.join(root, "new/deep/a")), Buffer.from("中\r\nno-eof"));
  const emptied = await handle({ action: "execute", root, tool: "write", arguments: { path: "a", content: "" } });
  assert.equal(emptied.ok, true, emptied.error);
  assert.equal((await bytes(root)).length, 0);
});

test("extension refuses an unsupported Pi API instead of replacing native edit", () => {
  assert.throws(() => createXnipExtension()({ on() {} }), /registerMcpServer/);
});


test("installed npm extension default resolves xnip/native and uses real Pi lifecycle",
  { skip: !process.env.XNIP_PI_TEST_PACKAGE }, async () => {
  const packageRoot = path.resolve(process.env.XNIP_PI_TEST_PACKAGE);
  const root = await fixture("old\n");
  const resolved = await handle({ action: "resolve-package", root, packageRoot });
  assert.equal(resolved.ok, true, resolved.error);
  assert.equal(resolved.resources.extensions.length, 1);
  const extensionPath = resolved.resources.extensions[0].path;
  assert.equal(path.resolve(extensionPath), path.join(packageRoot, "integrations/pi/index.mjs"));
  const described = await handle({ action: "describe", mode: "xnip-mcp", root, extensionPath });
  assert.equal(described.ok, true, described.error);
  assert.equal(described.registration.length, 1);
  const config = described.registration[0].config;
  assert.ok(path.isAbsolute(config.command), "default resolver must provide an absolute executable");
  assert.equal(config.env.XNIP_AUTO_UPDATE, "0");
  assert.ok(!config.command.split(/[\\/]/).includes("node_modules"),
    "execution cache must not execute or mutate an npm native payload");
  assert.equal(described.tools.xnip_edit.name, "mcp__xnip__xnip_edit");
  const result = await handle({ action: "execute", mode: "xnip-mcp", root, extensionPath,
    tool: "xnip_edit", arguments: { file: "a", old: "old", text: "new" } });
  assert.equal(result.ok, true, result.error);
  assert.deepEqual(await bytes(root), Buffer.from("new\n"));
  const stopped = await handle({ action: "lifecycle", root, extensionPath });
  assert.equal(stopped.ok, true, stopped.error);
  assert.equal(stopped.remaining, 0);
  assert.equal(stopped.connection_state, "closed");
});

test("real Pi registration, stdio discovery, tool projection, errors and shutdown", async () => {
  const root = await fixture("old\n");
  await fs.access(executable);
  const described = await handle({ action: "describe", mode: "xnip-mcp", root, xnipExecutable: executable });
  assert.equal(described.ok, true, described.error);
  assert.equal(described.registration.length, 1);
  assert.equal(described.registration[0].config.exposure, "direct");
  assert.equal(described.registration[0].config.env.XNIP_AUTO_UPDATE, "0");
  assert.equal(described.registration[0].config.cwd, root);
  assert.equal(described.tools.xnip_edit.name, "mcp__xnip__xnip_edit");
  assert.ok(!described.tools.edit);
  const preview = await handle({ action: "execute", mode: "xnip-mcp", root, xnipExecutable: executable,
    tool: "xnip_edit", arguments: { file: "a", old: "old", text: "new", dry_run: true } });
  assert.equal(preview.ok, true, preview.error);
  assert.match(preview.output, /written=false/);
  assert.deepEqual(await bytes(root), Buffer.from("old\n"));
  const written = await handle({ action: "execute", mode: "xnip-mcp", root, xnipExecutable: executable,
    tool: "xnip_edit", arguments: { file: "a", old: "old", text: "new" } });
  assert.equal(written.ok, true, written.error);
  assert.match(written.output, /written/);
  assert.deepEqual(await bytes(root), Buffer.from("new\n"));
  const rejected = await handle({ action: "execute", mode: "xnip-mcp", root, xnipExecutable: executable,
    tool: "xnip_edit", arguments: { file: "a", old: "missing", text: "bad" } });
  assert.equal(rejected.ok, false);
  assert.deepEqual(await bytes(root), Buffer.from("new\n"));
  const lifecycle = await handle({ action: "lifecycle", root, xnipExecutable: executable });
  assert.equal(lifecycle.ok, true, lifecycle.error);
  assert.equal(lifecycle.registered, 1);
  assert.equal(lifecycle.remaining, 0);
  assert.equal(lifecycle.connection_state, "closed");
});

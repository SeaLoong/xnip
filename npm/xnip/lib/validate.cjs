"use strict";
// Both parent and native packages carry this validator. It never executes a
// payload, downloads files, authenticates or publishes.
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const base = path.resolve(__dirname, "..");
const repository = "https://github.com/SeaLoong/xnip.git";
const platforms = {
  "linux-x64": ["x86_64-unknown-linux-musl", "xnip-linux-x64-musl"],
  "linux-arm64": ["aarch64-unknown-linux-musl", "xnip-linux-arm64-musl"],
  "darwin-x64": ["x86_64-apple-darwin", "xnip-darwin-x64"],
  "darwin-arm64": ["aarch64-apple-darwin", "xnip-darwin-arm64"],
  "win32-x64": ["x86_64-pc-windows-msvc", "xnip-win32-x64"],
  "win32-arm64": ["aarch64-pc-windows-msvc", "xnip-win32-arm64"],
};
const maxExecutable = 128 * 1024 * 1024;
const hex = (value, width) => typeof value === "string" && new RegExp("^[a-f0-9]{" + width + "}$").test(value);
const hash = data => crypto.createHash("sha256").update(data).digest("hex");
const fail = message => { throw new Error("xnip prepack: " + message); };
function ordinary(relative) {
  if (typeof relative !== "string" || !relative || relative.includes("\0")
      || path.isAbsolute(relative) || relative.split(/[\\/]/).includes("..")) fail("unsafe package path");
  const file = path.resolve(base, relative);
  const stat = fs.lstatSync(file);
  if (!stat.isFile() || stat.isSymbolicLink()) fail("expected ordinary file: " + relative);
  return { file, stat };
}
function json(relative) {
  return JSON.parse(fs.readFileSync(ordinary(relative).file, "utf8"));
}
function exactKeys(value, keys) {
  return value && typeof value === "object" && !Array.isArray(value)
    && Object.keys(value).sort().join("|") === [...keys].sort().join("|");
}
function validRelease(release, version) {
  if (!release || release.schema_version !== 1 || release.version !== version
      || !hex(release.source_commit, 40) || !exactKeys(release.archives, Object.keys(platforms))) fail("invalid six-platform release provenance");
  for (const [key, [target, name]] of Object.entries(platforms)) {
    const entry = release.archives[key];
    if (!entry || entry.target !== target || entry.package !== name
        || !hex(entry.archive_sha256, 64) || !hex(entry.sha256, 64)) fail("invalid release archive provenance: " + key);
  }
}
function publication(metadata, pkg) {
  if (metadata.schema_version !== 2 || metadata.version !== pkg.version
      || typeof metadata.localOnly !== "boolean") fail("invalid preparation identity");
  if (metadata.publication === "public-release") {
    if (pkg.private !== false || metadata.localOnly !== false
        || pkg.publishConfig?.access !== "public" || pkg.bundledDependencies || pkg.bundleDependencies) fail("public release cannot contain private or host-local metadata");
    validRelease(metadata.release, pkg.version);
    return true;
  }
  if (metadata.publication !== "private-local" || pkg.private !== true || metadata.release !== null) fail("private flag does not authorize a public release");
  return false;
}
function identity(entry, key, localOnly, isPublic, release) {
  const expected = platforms[key];
  if (!expected) fail("unknown platform " + key);
  const gnuLocal = localOnly && key.startsWith("linux-")
    && entry.target === expected[0].replace("-musl", "-gnu")
    && entry.package === "xnip-" + key + "-gnu-local";
  if (!gnuLocal && (entry.target !== expected[0] || entry.package !== expected[1])) fail("platform package/target mismatch: " + key);
  const binary = key.startsWith("win32-") ? "xnip.exe" : "xnip";
  if (entry.binary !== binary || !hex(entry.sha256, 64)
      || !Number.isSafeInteger(entry.size) || entry.size <= 0 || entry.size > maxExecutable) fail("invalid executable identity: " + key);
  if (localOnly ? entry.source !== "local-host-build" : entry.source !== undefined) fail("invalid local build provenance");
  if (isPublic) {
    const archive = release.archives[key];
    if (!hex(entry.archive_sha256, 64) || entry.archive_sha256 !== archive.archive_sha256
        || entry.sha256 !== archive.sha256 || entry.source_commit !== release.source_commit
        || gnuLocal || entry.format === "dynamic ELF (host-local only)") fail("public payload does not match release provenance");
  }
}
function binaryFormat(data, key, allowDynamic) {
  const arch = key.split("-")[1];
  if (key.startsWith("win32-")) {
    if (data.length < 64 || data.toString("latin1", 0, 2) !== "MZ") fail("not a PE executable");
    const offset = data.readUInt32LE(60);
    if (offset + 6 > data.length || data.toString("latin1", offset, offset + 4) !== "PE\0\0") fail("invalid PE header");
    if (data.readUInt16LE(offset + 4) !== (arch === "x64" ? 0x8664 : 0xaa64)) fail("PE architecture mismatch");
    return "PE";
  }
  if (key.startsWith("darwin-")) {
    if (data.length < 32 || data.readUInt32LE(0) !== 0xfeedfacf) fail("not a Mach-O 64 executable");
    if (data.readUInt32LE(4) !== (arch === "x64" ? 0x1000007 : 0x100000c)) fail("Mach-O architecture mismatch");
    return "Mach-O";
  }
  if (data.length < 64 || data.toString("latin1", 0, 6) !== "\x7fELF\x02\x01") fail("not a little-endian ELF 64 executable");
  if (data.readUInt16LE(18) !== (arch === "x64" ? 62 : 183)) fail("ELF architecture mismatch");
  const integer64 = offset => {
    const value = data.readBigUInt64LE(offset);
    if (value > BigInt(Number.MAX_SAFE_INTEGER)) fail("oversized ELF offset");
    return Number(value);
  };
  const offset = integer64(32), width = data.readUInt16LE(54), count = data.readUInt16LE(56);
  if (width < 56 || count > 1024 || offset + width * count > data.length) fail("invalid ELF program headers");
  let dynamic = false;
  for (let index = 0; index < count; index++) {
    const header = offset + width * index, kind = data.readUInt32LE(header);
    if (kind === 3) dynamic = true;
    if (kind === 2) {
      const start = integer64(header + 8), size = integer64(header + 32);
      if (start + size > data.length || size % 16) fail("invalid ELF dynamic table");
      for (let at = start; at < start + size; at += 16) {
        if (data.readBigInt64LE(at) === 1n) dynamic = true;
      }
    }
  }
  if (dynamic && !allowDynamic) fail("Linux distribution payload must be static, without interpreter/shared dependencies");
  return dynamic ? "dynamic ELF (host-local only)" : "static ELF";
}
const pkg = json("package.json");
if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.+-]+)?$/.test(pkg.version)
    || pkg.repository?.type !== "git" || pkg.repository?.url !== repository
    || !exactKeys(pkg.scripts, ["prepack"]) || pkg.scripts.prepack !== "node lib/validate.cjs") fail("invalid package version/repository/prepack metadata");
const native = fs.existsSync(path.join(base, "binary.json"));
const metadata = json(native ? "binary.json" : "native-manifest.json");
const isPublic = publication(metadata, pkg);
for (const relative of ["LICENSE", "THIRD_PARTY_LICENSES.md"]) ordinary(relative);
if (native) {
  const keys = Object.keys(platforms).filter(key => pkg.name === platforms[key][1]
    || pkg.name === "xnip-" + key + "-gnu-local");
  if (keys.length !== 1) fail("invalid native package name");
  const key = keys[0], [os, cpu] = key.split("-");
  identity(metadata, key, metadata.localOnly, isPublic, metadata.release);
  if (JSON.stringify(pkg.files) !== JSON.stringify([metadata.binary,"binary.json","lib/validate.cjs","LICENSE","THIRD_PARTY_LICENSES.md"])) fail("native package file allowlist mismatch");
  if (metadata.package !== pkg.name || !exactKeys(pkg.exports, ["./package.json"])
      || pkg.exports["./package.json"] !== "./package.json"
      || JSON.stringify(pkg.os) !== JSON.stringify([os]) || JSON.stringify(pkg.cpu) !== JSON.stringify([cpu])) fail("native package platform metadata mismatch");
  const { file, stat } = ordinary(metadata.binary);
  if (stat.size !== metadata.size || stat.size > maxExecutable) fail("native executable size mismatch");
  const bytes = fs.readFileSync(file);
  if (hash(bytes) !== metadata.sha256) fail("native executable checksum mismatch");
  const markers = new Set([...bytes.toString("latin1").matchAll(/\x00XNIP_BUILD_ID_V1:([0-9A-Za-z.+-]+)\|([0-9A-Za-z_-]+)\x00/g)]
    .map(match => match[1] + "|" + match[2]));
  if (markers.size !== 1 || !markers.has(pkg.version + "|" + metadata.target)) fail("binary build identity mismatch");
  const format = binaryFormat(bytes, key, metadata.localOnly && metadata.target.endsWith("-gnu"));
  if (format !== metadata.format) fail("executable format metadata mismatch");
} else {
  if (JSON.stringify(pkg.files) !== JSON.stringify(["bin/","lib/","integrations/","native-manifest.json","README.md","LICENSE","THIRD_PARTY_LICENSES.md"])) fail("parent package file allowlist mismatch");
  if (pkg.name !== "xnip" || metadata.repository?.url !== repository || metadata.repository?.type !== "git") fail("invalid parent package identity");
  const present = Object.keys(metadata.platforms || {});
  if (present.length !== (metadata.localOnly ? 1 : 6)
      || present.some(key => !platforms[key])) fail("invalid prepared platform set");
  const expectedDependencies = {};
  for (const key of present) {
    const entry = metadata.platforms[key];
    identity(entry, key, metadata.localOnly, isPublic, metadata.release);
    expectedDependencies[entry.package] = pkg.version;
  }
  if (!exactKeys(pkg.optionalDependencies, Object.keys(expectedDependencies))
      || Object.entries(expectedDependencies).some(([name, version]) => pkg.optionalDependencies[name] !== version)) fail("optional dependencies must pin every platform exactly");
  if (metadata.localOnly && JSON.stringify(pkg.bundledDependencies) !== JSON.stringify(Object.keys(expectedDependencies))) fail("private host-local package must bundle its one payload");
  const required = {
    "./native": "./lib/native.cjs", "./dsh": "./integrations/dsh/index.mjs",
    "./pi": "./integrations/pi/index.mjs", "./package.json": "./package.json",
  };
  if (pkg.bin?.xnip !== "./bin/xnip.cjs" || !exactKeys(pkg.exports, Object.keys(required))
      || Object.entries(required).some(([key, value]) => pkg.exports[key] !== value)
      || pkg.dsh?.bundle?.patch !== "./integrations/dsh/cordis.patch.yml"
      || JSON.stringify(pkg.pi?.extensions) !== '["./integrations/pi/index.mjs"]') fail("invalid CLI or harness entry points");
  for (const relative of [pkg.bin.xnip, ...Object.values(required), pkg.dsh.bundle.patch]) ordinary(relative);
}

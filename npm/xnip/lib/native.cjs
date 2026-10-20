"use strict";

const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const crypto = require("node:crypto");
const manifestPath = path.join(__dirname, "..", "native-manifest.json");

function digest(file) {
  return crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}
function ordinary(file, directory = false) {
  const stat = fs.lstatSync(file);
  if (stat.isSymbolicLink() || (directory ? !stat.isDirectory() : !stat.isFile())) {
    throw new Error("xnip: expected an ordinary " + (directory ? "directory: " : "file: ") + file);
  }
  return stat;
}
function readManifest() {
  let manifest;
  try { manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8")); }
  catch (error) { throw new Error("xnip: package is not prepared; use the repository npm packaging command. " + error.message); }
  const version = require("../package.json").version;
  if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.+-]+)?$/.test(version) || manifest.version !== version || !manifest.platforms) throw new Error("xnip: native manifest version mismatch");
  return manifest;
}
function platformKey(platform = process.platform, arch = process.arch) {
  const key = platform + "-" + arch;
  if (!["linux-x64", "linux-arm64", "darwin-x64", "darwin-arm64", "win32-x64", "win32-arm64"].includes(key)) {
    throw new Error("xnip: unsupported platform " + key);
  }
  return key;
}
function cacheDirectory() {
  const base = process.env.XNIP_NPM_CACHE_DIR || (process.platform === "win32"
    ? path.join(process.env.LOCALAPPDATA || os.tmpdir(), "xnip", "npm-exec")
    : path.join(process.env.XDG_CACHE_HOME || path.join(os.homedir(), ".cache"), "xnip", "npm-exec"));
  return path.resolve(base);
}
function verify(file, entry) {
  const stat = ordinary(file);
  if (stat.size !== entry.size || digest(file) !== entry.sha256) {
    throw new Error("xnip: executable integrity check failed: " + file);
  }
}
function resolveNativeExecutable() {
  const manifest = readManifest();
  const key = platformKey();
  const entry = manifest.platforms[key];
  if (!entry || !/^[a-f0-9]{64}$/.test(entry.sha256) || !Number.isSafeInteger(entry.size) || entry.size <= 0) {
    throw new Error("xnip: invalid native manifest entry for " + key);
  }
  let payloadRoot;
  try { payloadRoot = path.dirname(require.resolve(entry.package + "/package.json")); }
  catch { throw new Error("xnip: missing optional platform package " + entry.package + ". Install optional dependencies; no runtime download is performed."); }
  ordinary(payloadRoot, true);
  const metadata = JSON.parse(fs.readFileSync(path.join(payloadRoot, "binary.json"), "utf8"));
  if (metadata.version !== manifest.version || metadata.target !== entry.target || metadata.sha256 !== entry.sha256) {
    throw new Error("xnip: platform package does not match the parent package");
  }
  const filename = process.platform === "win32" ? "xnip.exe" : "xnip";
  if (entry.binary !== filename) throw new Error("xnip: invalid executable name");
  const source = path.join(payloadRoot, filename);
  verify(source, entry);
  const root = cacheDirectory();
  fs.mkdirSync(root, { recursive: true, mode: 0o700 });
  ordinary(root, true);
  const destination = path.join(root, manifest.version + "-" + key + "-" + entry.sha256);
  const executable = path.join(destination, filename);
  if (fs.existsSync(destination)) {
    ordinary(destination, true);
    verify(executable, entry);
    return executable;
  }
  // Publish a complete nonempty directory atomically. Existing cache entries are
  // never replaced, and running images never map the package-manager payload.
  const staged = fs.mkdtempSync(path.join(root, ".prepare-"));
  const stagedExecutable = path.join(staged, filename);
  try {
    fs.copyFileSync(source, stagedExecutable, fs.constants.COPYFILE_EXCL);
    fs.chmodSync(stagedExecutable, 0o700);
    verify(stagedExecutable, entry);
    const handle = fs.openSync(stagedExecutable, "r+");
    try { fs.fsyncSync(handle); } finally { fs.closeSync(handle); }
    try { fs.renameSync(staged, destination); }
    catch (error) {
      if (!fs.existsSync(destination)) throw error;
      ordinary(destination, true);
      verify(executable, entry);
    }
    verify(executable, entry);
    return executable;
  } finally {
    // Only these files, in our unique preparation directory, are disposable.
    if (fs.existsSync(stagedExecutable)) fs.unlinkSync(stagedExecutable);
    if (fs.existsSync(staged)) fs.rmdirSync(staged);
  }
}
function childEnvironment(environment = process.env) {
  return { ...environment, XNIP_AUTO_UPDATE: "0" };
}
module.exports = { resolveNativeExecutable, platformKey, childEnvironment };

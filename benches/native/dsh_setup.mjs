#!/usr/bin/env node
/** Prepare/check the isolated official DSH executor. No model, UI or lifecycle scripts. */
import fs from "node:fs/promises"
import path from "node:path"
import { fileURLToPath, pathToFileURL } from "node:url"
import { createRequire } from "node:module"
import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"

export const COMMIT = "687f282e8625c6b8d11ff58211a90524be2e4317"
export const VERSION = "0.2.1-rc.2"
export const UPSTREAM = "https://github.com/deepseek-ai/deepseek-harness.git"
const HERE = path.dirname(fileURLToPath(import.meta.url))
const ROOT = path.resolve(HERE, "../..")
export const CACHE = path.join(ROOT, "target/native-bench/dsh-" + VERSION)
const hash = bytes => createHash("sha256").update(bytes).digest("hex")
const git = (source, ...args) => execFileSync("git", ["-C", source, ...args], { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 })
const slash = value => value.replaceAll("\\", "/")

async function walk(directory, base = directory) {
  const rows = []
  for (const item of await fs.readdir(directory, { withFileTypes: true })) {
    const full = path.join(directory, item.name)
    if (item.isDirectory()) rows.push(...await walk(full, base))
    else if (item.isFile() && /\.(?:[cm]?js|json|node)$/.test(item.name)) {
      const bytes = await fs.readFile(full)
      rows.push({ file: slash(path.relative(base, full)), bytes: bytes.length, sha256: hash(bytes) })
    }
  }
  return rows.sort((a, b) => a.file.localeCompare(b.file, "en"))
}

async function verifyFiles(base, files) {
  for (const item of files) {
    const bytes = await fs.readFile(path.join(base, item.file))
    if (hash(bytes) !== item.sha256) throw new Error("DSH cache hash mismatch: " + item.file)
  }
}

export async function verifyPrepared(directory = CACHE) {
  const metadata = JSON.parse(await fs.readFile(path.join(directory, "metadata.json"), "utf8"))
  if (metadata.commit !== COMMIT || metadata.version !== VERSION) throw new Error("DSH cache revision mismatch")
  const lock = await fs.readFile(path.join(HERE, "dsh_package-lock.json"))
  if (hash(lock) !== metadata.dependency_lock_sha256) throw new Error("DSH dependency lock changed; prepare again")
  for (const file of ["dsh_setup.mjs", "dsh_runtime.ts", "dsh_package.json"]) {
    if (hash(await fs.readFile(path.join(HERE, file))) !== metadata.build_inputs[file]) throw new Error("DSH adapter build input changed: " + file)
  }
  if (hash(await fs.readFile(path.join(directory, "runtime.mjs"))) !== metadata.runtime_bundle_sha256) throw new Error("DSH runtime bundle hash mismatch")
  await verifyFiles(path.join(directory, "sources"), metadata.source_files)
  await verifyFiles(path.join(directory, "node_modules"), metadata.dependency_files)
  return metadata
}

export async function prepare({ directory = CACHE, source } = {}) {
  const [major, minor] = process.versions.node.split(".").map(Number)
  if (major < 22 || major === 23 || major === 22 && minor < 19) throw new Error("DSH requires Node 22.19+ or 24+")
  await fs.mkdir(directory, { recursive: true })
  source ??= path.join(directory, "upstream")
  try { await fs.access(path.join(source, ".git")) }
  catch {
    execFileSync("git", ["clone", "--depth", "1", "--filter=blob:none", "--no-checkout", UPSTREAM, source], { stdio: ["ignore", "pipe", "pipe"] })
    if (git(source, "rev-parse", "HEAD").trim() !== COMMIT) {
      execFileSync("git", ["-C", source, "fetch", "--depth", "1", "origin", COMMIT], { stdio: ["ignore", "pipe", "pipe"] })
      execFileSync("git", ["-C", source, "update-ref", "HEAD", COMMIT])
    }
  }
  if (git(source, "rev-parse", "HEAD").trim() !== COMMIT) throw new Error("Existing DSH source is not the fixed commit; refusing to reset it")
  if (git(source, "remote", "get-url", "origin").trim().replace(/\.git$/, "") !== UPSTREAM.replace(/\.git$/, "")) throw new Error("DSH source origin is not official")
  let checkedOut = false
  try { await fs.access(path.join(source, ".git/index")); checkedOut = true } catch {}
  if (checkedOut && git(source, "diff", "--name-only", "HEAD", "--").trim()) throw new Error("DSH source checkout has local changes")
  const paths = new Set(git(source, "ls-tree", "-r", "--name-only", COMMIT).trim().split("\n"))
  const manifests = [...paths].filter(file => file.endsWith("/package.json") && file.startsWith("packages/"))
  const copies = new Map()
  const blob = async file => {
    if (!paths.has(file)) throw new Error("No fixed DSH source: " + file)
    if (copies.has(file)) return copies.get(file)
    const bytes = execFileSync("git", ["-C", source, "show", COMMIT + ":" + file], { maxBuffer: 16 * 1024 * 1024 })
    copies.set(file, bytes)
    const dest = path.join(directory, "sources", file)
    await fs.mkdir(path.dirname(dest), { recursive: true })
    await fs.writeFile(dest, bytes)
    return bytes
  }
  const packageCache = new Map()
  const packageEntry = async request => {
    const pieces = request.split("/")
    const name = pieces.slice(0, 2).join("/")
    const subpath = pieces.slice(2).join("/")
    if (!packageCache.has(name)) {
      const suffix = name.replace("@deepseek-ai/dsh-", "")
      const candidates = manifests.filter(file => suffix === path.posix.basename(path.posix.dirname(file)) || suffix.endsWith("-" + path.posix.basename(path.posix.dirname(file))))
      let found
      for (const manifest of candidates) {
        const json = JSON.parse((await blob(manifest)).toString("utf8"))
        if (json.name === name) { found = { root: path.posix.dirname(manifest), json }; break }
      }
      if (!found) throw new Error("Unknown official DSH package: " + name)
      packageCache.set(name, found)
    }
    const { root, json } = packageCache.get(name)
    if (!subpath) return root + "/src/index.ts"
    if (subpath.startsWith("src/")) return root + "/" + subpath
    const exported = json.exports?.["./" + subpath]
    const target = typeof exported === "string" ? exported : exported?.default
    if (!target) throw new Error("Unknown DSH subpath: " + request)
    return root + "/" + target.replace(/^\.\//, "").replace(/^lib\/(?:types\/)?/, "src/").replace(/\.js$/, ".ts")
  }
  await fs.copyFile(path.join(HERE, "dsh_package.json"), path.join(directory, "package.json"))
  const lockPath = path.join(HERE, "dsh_package-lock.json")
  const lock = await fs.readFile(lockPath)
  await fs.copyFile(lockPath, path.join(directory, "package-lock.json"))
  const npm = path.join(path.dirname(process.execPath), "node_modules/npm/bin/npm-cli.js")
  execFileSync(process.execPath, [npm, "ci", "--prefix", directory, "--ignore-scripts", "--legacy-peer-deps", "--no-audit", "--no-fund"], {
    cwd: ROOT, stdio: ["ignore", "pipe", "pipe"], timeout: 180000,
  })
  const require = createRequire(path.join(directory, "package.json"))
  const esbuild = await import(pathToFileURL(require.resolve("esbuild")).href)
  const resolveSource = input => {
    const candidate = slash(path.posix.normalize(input))
    for (const file of [candidate, candidate.replace(/\.js$/, ".ts"), candidate + ".ts", candidate + "/index.ts"]) if (paths.has(file)) return file
    throw new Error("Unknown DSH relative source: " + candidate)
  }
  await esbuild.build({
    entryPoints: [path.join(HERE, "dsh_runtime.ts")], outfile: path.join(directory, "runtime.mjs"),
    bundle: true, platform: "node", format: "esm", target: "node22.19", logLevel: "silent",
    plugins: [{ name: "fixed-official-dsh-source", setup(build) {
      build.onResolve({ filter: /^@deepseek-ai\/dsh-/ }, async args => ({ path: resolveSource(await packageEntry(args.path)), namespace: "dsh-source" }))
      build.onResolve({ filter: /.*/, namespace: "dsh-source" }, args => {
        if (args.path.startsWith(".")) return { path: resolveSource(path.posix.join(path.posix.dirname(args.importer), args.path)), namespace: "dsh-source" }
        if (!args.path.startsWith("node:")) require.resolve(args.path)
        return { path: args.path, external: true }
      })
      build.onResolve({ filter: /^(?:@deepseek-ai\/(?:cordis|schemastery)|zod)(?:\/|$)/ }, args => ({ path: args.path, external: true }))
      build.onLoad({ filter: /.*/, namespace: "dsh-source" }, async args => {
        const contents = (await blob(args.path)).toString("utf8")
        if (args.path.endsWith(".json")) return { contents, loader: "json" }
        // Bundled code must resolve its own package/resources beside the fixed
        // original module, not beside runtime.mjs. This is module-location
        // preservation only; matching, policy and filesystem bodies stay native.
        const transformed = await esbuild.transform(contents, {
          loader: "ts", target: "node22.19", format: "esm",
          define: { "import.meta.url": JSON.stringify(pathToFileURL(path.join(directory, "sources", args.path)).href) },
        })
        return { contents: transformed.code, loader: "js" }
      })
    }}],
  })
  const sourceFiles = [...copies].map(([file, bytes]) => ({ file, bytes: bytes.length, sha256: hash(bytes), source_url: "https://github.com/deepseek-ai/deepseek-harness/blob/" + COMMIT + "/" + file })).sort((a, b) => a.file.localeCompare(b.file, "en"))
  const buildInputs = {}
  for (const file of ["dsh_setup.mjs", "dsh_runtime.ts", "dsh_package.json"]) buildInputs[file] = hash(await fs.readFile(path.join(HERE, file)))
  const metadata = {
    backend: "deepseek-harness", version: VERSION, commit: COMMIT, source_repository: UPSTREAM,
    execution_level: "official ToolRuntime and filesystem executors; no model or UI",
    source_files: sourceFiles, dependency_lock_sha256: hash(lock), build_inputs: buildInputs,
    dependency_files: await walk(path.join(directory, "node_modules")),
    runtime_bundle_sha256: hash(await fs.readFile(path.join(directory, "runtime.mjs"))),
    source_adaptations: ["TypeScript transpilation and bundling; import.meta.url rebased to each copied original module for native relative package/resource lookup. No editing, policy, schema or IO stubs."],
    model_calls: 0, install_scripts_executed: false, runtime: { executable: process.execPath, version: process.version },
  }
  await fs.writeFile(path.join(directory, "metadata.json"), JSON.stringify(metadata, null, 2) + "\n")
  return metadata
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2)
    const sourceIndex = args.indexOf("--source")
    const result = args.includes("--check") ? await verifyPrepared() : await prepare({ source: sourceIndex < 0 ? undefined : path.resolve(args[sourceIndex + 1]) })
    console.log(JSON.stringify({ status: "available", commit: result.commit, version: result.version, source_files: result.source_files.length, runtime_bundle_sha256: result.runtime_bundle_sha256 }))
  } catch (error) { console.error(error.stack ?? String(error)); process.exitCode = 1 }
}

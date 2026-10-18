/**
 * Non-model context adapters for the pinned OpenCode tool executor.
 * Matching, patch parsing, BOM handling, filesystem mutation, schema validation,
 * and output truncation are imported unchanged from OpenCode.
 */
import { Context, Effect } from "effect"
import path from "node:path"
import os from "node:os"

export class Instance extends Context.Service<Instance, {directory: string; worktree: string}>()("native-bench/instance") {}
class LspService extends Context.Service<LspService, any>()("native-bench/lsp") {}
class FormatService extends Context.Service<FormatService, any>()("native-bench/format") {}
class EventService extends Context.Service<EventService, any>()("native-bench/events") {}
class AgentService extends Context.Service<AgentService, any>()("native-bench/agent") {}
class ConfigService extends Context.Service<ConfigService, any>()("native-bench/config") {}

export const InstanceState = { context: Instance }
export const LSP = { Service: LspService, Diagnostic: { report: () => "" } }
export const Format = { Service: FormatService }
export const EventV2Bridge = { Service: EventService }
export const Agent = { Service: AgentService }
export const Config = { Service: ConfigService }
export const FileSystem = { Event: { Edited: "file.edited" } }
export const Watcher = { Event: { Updated: "file.watcher.updated" } }
export const filesystem = undefined
export const serviceUse = () => () => { throw new Error("unused serviceUse adapter invoked") }
export const makeGlobalNode = (options: any) => options
export const LayerNode = { make: (options: any) => options }
export const LocalContext = { create: () => ({}) }
export const evaluate = () => { throw new Error("unused permission evaluator invoked") }
let serial = 0
export const ToolID = { ascending: () => "tool_native_benchmark_" + (++serial) }
export const TRUNCATION_DIR = path.join(os.tmpdir(), "xnip-native-opencode-truncation-" + process.pid)
const unavailableGlob = () => { throw new Error("glob is outside this edit benchmark") }
export const Glob = { scan: unavailableGlob, match: unavailableGlob }

export function provideContext(effect: any, root: string, events: any[]) {
  return effect.pipe(
    Effect.provideService(Instance, { directory: root, worktree: root }),
    Effect.provideService(LspService, {
      touchFile: () => Effect.void,
      diagnostics: () => Effect.succeed({}),
    }),
    Effect.provideService(FormatService, { file: () => Effect.succeed(false) }),
    Effect.provideService(EventService, {
      publish: (event: string, data: any) => Effect.sync(() => { events.push({ event, data }) }),
    }),
    Effect.provideService(AgentService, { get: () => Effect.succeed({ name: "native-benchmark" }) }),
  )
}

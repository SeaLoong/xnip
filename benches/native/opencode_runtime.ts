import { Effect, Schema } from "effect"
import { NodeFileSystem } from "@effect/platform-node"
import { EditTool, Parameters as EditParameters } from "native-opencode/edit"
import { ApplyPatchTool, Parameters as PatchParameters } from "native-opencode/apply_patch"
import { FSUtil } from "native-opencode/fs-util"
import * as Tool from "native-opencode/tool"
import * as Truncate from "native-opencode/truncate"
import { provideContext } from "./opencode_stubs"

function run(effect: any, root: string, events: any[]) {
  return Effect.runPromise(Effect.scoped(provideContext(effect, root, events).pipe(
    Effect.provide(Truncate.node.layer),
    Effect.provide(FSUtil.node.layer),
    Effect.provide(NodeFileSystem.layer),
  )))
}

function toolInfo(kind: string) {
  const nativeTool = kind === "apply_patch" ? ApplyPatchTool : EditTool
  return Effect.gen(function* () { return yield* Tool.init(yield* nativeTool) })
}

export async function describe() {
  const edit = await run(toolInfo("edit"), process.cwd(), [])
  const patch = await run(toolInfo("apply_patch"), process.cwd(), [])
  return {
    edit: { name: "edit", description: edit.description, schema: Schema.toJsonSchemaDocument(EditParameters) },
    apply_patch: { name: "apply_patch", description: patch.description, schema: Schema.toJsonSchemaDocument(PatchParameters) },
  }
}

export async function execute(request: any) {
  const events: any[] = []
  const permissionRequests: any[] = []
  const progressMetadata: any[] = []
  const ctx = {
    sessionID: "session_native_benchmark",
    messageID: "message_native_benchmark",
    agent: "native-benchmark",
    abort: new AbortController().signal,
    messages: [],
    ask: (input: any) => Effect.sync(() => { permissionRequests.push(input) }),
    metadata: (input: any) => Effect.sync(() => { progressMetadata.push(input) }),
  }
  const start = performance.now()
  try {
    const result = await run(Effect.gen(function* () {
      const tool = yield* toolInfo(request.tool ?? "edit")
      return yield* tool.execute(request.arguments, ctx)
    }), request.root, events)
    return { ok: true, output: result.output, native_result: result, elapsed_ms: performance.now() - start,
      metadata: { events, permission_requests: permissionRequests, progress_metadata: progressMetadata } }
  } catch (error: any) {
    return { ok: false, output: "", error: error?.message ?? String(error),
      native_error: { name: error?.name, message: error?.message, stack: error?.stack },
      elapsed_ms: performance.now() - start,
      metadata: { events, permission_requests: permissionRequests, progress_metadata: progressMetadata } }
  }
}

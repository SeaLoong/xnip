/** Compact model projections of official tools; execution and UI stay host-owned. */
const descriptions = {
  read: 'Read UTF-8 text with line numbers. Read before editing.',
  write: 'Create or replace a UTF-8 file. Read an existing file first.',
  edit: 'Replace literal text. old_string must match exactly once unless replace_all is true.',
};
const argumentsHelp = {
  file_path: 'File path.', old_string: 'Exact text to replace.',
  new_string: 'Replacement; empty deletes.', content: 'Complete UTF-8 content.',
  offset: 'First line (1-based).', limit: 'Maximum lines.',
  replace_all: 'Replace every match; default false.',
};

export function compactDefinition(tool) {
  if (!descriptions[tool.name]) return tool;
  const parameters = structuredClone(tool.parameters);
  for (const [name, property] of Object.entries(parameters.properties ?? {})) {
    if (argumentsHelp[name]) property.description = argumentsHelp[name];
  }
  return { ...tool, description: descriptions[tool.name], parameters,
    output: tool.name === 'read' ? tool.output : { ...tool.output,
      render: (_args, value) => [{ type: 'text', text:
        tool.name === 'edit' ? 'edited' : value.operation ?? 'written' }],
    },
  };
}

export function createCompactToolsPlugin(loadTools = () => import('@deepseek-ai/dsh-tool-fs')) {
  return {
    name: 'xnip-compact-tools', inject: ['tools', 'fs', 'systemPrompt', 'workingDirectory'],
    async apply(ctx, config = {}) {
      const official = await loadTools();
      // Adapt registration at the official plugin boundary. Execution closures
      // retain the live context and its policy services.
      const tools = new Proxy(ctx.tools, { get(target, key) {
        if (key === 'register') return (definition, options) => target.register(compactDefinition(definition), options);
        return Reflect.get(target, key);
      } });
      const context = new Proxy(ctx, { get(target, key, receiver) {
        return key === 'tools' ? tools : Reflect.get(target, key, receiver);
      } });
      official.apply(context, official.Config(config));
    },
  };
}
const plugin = createCompactToolsPlugin();
export const { name, inject, apply } = plugin;
export default plugin;

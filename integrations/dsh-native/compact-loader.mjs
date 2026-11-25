import plugin from './compact-tools.mjs';

// Nested presets resolve module names from the host tree. Publish the local
// adapter there before Cordis starts any preset/group that references it.
export const inject = ['loader'];
export function apply(ctx) {
  const { name } = plugin;
  ctx.loader.builtins[name] = plugin;
  ctx.effect(() => () => { delete ctx.loader.builtins[name]; });
  ctx.provide('xnipCompactTools', plugin);
}

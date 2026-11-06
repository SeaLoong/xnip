// A static SDK import lets Pi's official extension loader bind its installed
// public SDK, including when xnip and Pi live in separate npm directories.
import * as host from '@earendil-works/pi-coding-agent';
import { createXnipNativeExtension } from './extension.mjs';

export { createXnipNativeExtension } from './extension.mjs';
export default createXnipNativeExtension({ loadHost: async () => host });

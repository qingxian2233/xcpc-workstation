// Build the React frontend used by Tauri. Tools are resolved the way Node resolves
// modules, so this works whether npm hoisted them to the root or pnpm kept them local.
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const cwd = fileURLToPath(new URL('../web/', import.meta.url));
const require = createRequire(new URL('../web/package.json', import.meta.url));
const bin = (pkg, file) => join(dirname(require.resolve(`${pkg}/package.json`)), file);
for (const [script, args] of [
  [bin('typescript', 'bin/tsc'), ['--noEmit']],
  [bin('vite', 'bin/vite.js'), ['build']],
]) {
  const result = spawnSync(process.execPath, [script, ...args], { cwd, stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

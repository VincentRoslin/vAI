#!/usr/bin/env node
// Create / refresh the Python worker venv at <repo>/.venv from
// workers/requirements.txt (ADR-0018). Idempotent. Needs `uv` on PATH or
// importable as `python -m uv` (install: `python -m pip install --user uv`).
//
//   node scripts/setup-venv.mjs
//
// The shipped app does NOT use this — it bundles an embedded CPython + the same
// frozen set (ADR-0014). This is the dev path only.

import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const venv = join(repo, '.venv');
const req = join(repo, 'workers', 'requirements.txt');

function run(cmd, args) {
  process.stdout.write(`$ ${cmd} ${args.join(' ')}\n`);
  execFileSync(cmd, args, { cwd: repo, stdio: 'inherit' });
}

// `uv` directly, else `python -m uv`.
let uv = ['uv'];
try {
  execFileSync('uv', ['--version'], { stdio: 'ignore' });
} catch {
  uv = [process.platform === 'win32' ? 'python' : 'python3', '-m', 'uv'];
}

if (!existsSync(venv)) {
  run(uv[0], [...uv.slice(1), 'venv', '.venv', '--python', '3.11']);
}
run(uv[0], [...uv.slice(1), 'pip', 'install', '--python', '.venv', '-r', req]);

const py = join(
  venv,
  process.platform === 'win32' ? 'Scripts' : 'bin',
  process.platform === 'win32' ? 'python.exe' : 'python',
);
run(py, [
  '-c',
  "import faster_whisper, ctranslate2; print('ct2 cuda devices:', ctranslate2.get_cuda_device_count())",
]);
process.stdout.write('venv ready at .venv\n');

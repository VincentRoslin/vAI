#!/usr/bin/env node
// Create / refresh the two Python venvs (ADR-0018 + the 2026-09-06 two-venv
// amendment). Idempotent. Needs `uv` (install: `python -m pip install --user uv`)
// — found on PATH or in Python's per-user Scripts dir automatically.
//
//   node scripts/setup-venv.mjs            # both
//   node scripts/setup-venv.mjs workers    # just <repo>/.venv
//   node scripts/setup-venv.mjs image      # just <repo>/.venv-image
//
//   .venv        — workers (STT/TTS): workers/requirements.txt + overrides.txt.
//   .venv-image  — image sidecar only: image_gen/requirements.txt + the torch
//                  line from workers/overrides.txt. Separate because
//                  chatterbox-tts 0.1.7 hard-pins torch==2.6.0 /
//                  transformers==5.2.0 / diffusers==0.29.0, unresolvable against
//                  Krea 2's transformers 5.16.1 + diffusers @ commit.
//
// The shipped app does NOT use either — it bundles an embedded CPython + the
// same frozen sets (ADR-0014). This is the dev path only.

import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const only = process.argv[2]; // undefined | "workers" | "image"

// Locate `uv`: on PATH, else the `uv.exe` a `pip install --user uv` drops into
// Python's per-user scripts dir (not on PATH by default; `python -m uv` does NOT
// work — uv ships as a binary with no importable module).
function findUv() {
  try {
    execFileSync('uv', ['--version'], { stdio: 'ignore' });
    return 'uv';
  } catch {
    /* not on PATH */
  }
  const py = process.platform === 'win32' ? 'python' : 'python3';
  try {
    const base = execFileSync(py, ['-c', 'import site;print(site.getuserbase())'], {
      encoding: 'utf8',
    }).trim();
    const cand =
      process.platform === 'win32' ? join(base, 'Scripts', 'uv.exe') : join(base, 'bin', 'uv');
    if (existsSync(cand)) return cand;
  } catch {
    /* fall through */
  }
  throw new Error(
    'uv not found. Install it and retry:\n' +
      '  python -m pip install --user uv\n' +
      'then add its Scripts dir to PATH, or just re-run this script (it looks ' +
      'there automatically).',
  );
}

const uvBin = findUv();

function run(cmd, args) {
  process.stdout.write(`$ ${cmd} ${args.join(' ')}\n`);
  execFileSync(cmd, args, { cwd: repo, stdio: 'inherit' });
}

function venvPython(dir) {
  return join(
    repo,
    dir,
    process.platform === 'win32' ? 'Scripts' : 'bin',
    process.platform === 'win32' ? 'python.exe' : 'python',
  );
}

// name: gitignored dir; reqs: -r files; overrides: --override file (optional).
function buildVenv({ dir, reqs, overrides, check }) {
  if (!existsSync(join(repo, dir))) {
    run(uvBin, ['venv', dir, '--python', '3.11']);
  }
  const args = ['pip', 'install', '--python', dir];
  for (const r of reqs) args.push('-r', r);
  if (overrides) args.push('--override', overrides);
  run(uvBin, args);
  run(venvPython(dir), ['-c', check]);
  process.stdout.write(`venv ready at ${dir}\n`);
}

if (only !== 'image') {
  buildVenv({
    dir: '.venv',
    reqs: [join('workers', 'requirements.txt')],
    overrides: join('workers', 'overrides.txt'),
    check:
      'import faster_whisper, ctranslate2, torch, chatterbox; ' +
      "print('ct2 cuda devices:', ctranslate2.get_cuda_device_count()); " +
      "print('torch cuda:', torch.cuda.is_available())",
  });
}

if (only !== 'workers') {
  buildVenv({
    dir: '.venv-image',
    reqs: [join('image_gen', 'requirements.txt')],
    // workers/overrides.txt carries the cu128 --extra-index-url + torch 2.11.0
    // pin; diffusers pulls torch, the override forces the Blackwell build.
    overrides: join('workers', 'overrides.txt'),
    check:
      'import diffusers, transformers, torch; from diffusers import Krea2Pipeline; ' +
      "print('diffusers', diffusers.__version__, 'transformers', transformers.__version__); " +
      "print('torch cuda:', torch.cuda.is_available())",
  });
}

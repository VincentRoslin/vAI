#!/usr/bin/env node
// Build the REAL (release) binary with the real settings and launch it in place
// — no vite dev server, no installer (Phase 18.5). The installer / clean-machine
// layout is Phase 37.
//
//   node scripts/deploy-local.mjs            build + launch
//   node scripts/deploy-local.mjs --no-launch   build + print paths only
//
// It uses your existing %APPDATA%\com.localai.app\config.json (which already
// points models / runtimes / workers at this repo). Logs and diagnostics land
// under that same folder.

import { execFileSync, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const noLaunch = process.argv.includes('--no-launch');
const isWin = process.platform === 'win32';
const IDENT = 'com.localai.app';

function run(cmd, args, opts = {}) {
  process.stdout.write(`$ ${cmd} ${args.join(' ')}\n`);
  // Windows needs a shell to resolve `npm` (a .cmd shim) under execFileSync.
  execFileSync(cmd, args, { cwd: repo, stdio: 'inherit', shell: isWin, ...opts });
}

function gitSha() {
  try {
    return execFileSync('git', ['rev-parse', '--short', 'HEAD'], { cwd: repo }).toString().trim();
  } catch {
    return '(no git)';
  }
}

function appDataDir() {
  if (isWin) return join(process.env.APPDATA ?? '', IDENT);
  if (process.platform === 'darwin')
    return join(process.env.HOME ?? '', 'Library', 'Application Support', IDENT);
  return join(process.env.XDG_DATA_HOME ?? join(process.env.HOME ?? '', '.local', 'share'), IDENT);
}

// 1. Frontend (embedded into the binary at build time via tauri.conf frontendDist).
run('npm', ['run', 'build']);

// 2. Release binary.
run('cargo', ['build', '--release', '--manifest-path', join(repo, 'src-tauri', 'Cargo.toml')]);

const exe = join(repo, 'src-tauri', 'target', 'release', isWin ? 'localai.exe' : 'localai');
if (!existsSync(exe)) {
  process.stderr.write(`build produced no binary at ${exe}\n`);
  process.exit(1);
}

const data = appDataDir();
const info = {
  git_sha: gitSha(),
  exe,
  config: join(data, 'config.json'),
  logs: join(data, 'logs'),
  diagnostics: join(data, 'diagnostics'),
};
process.stdout.write('\n=== LocalAI local deploy ===\n');
for (const [k, v] of Object.entries(info)) process.stdout.write(`  ${k.padEnd(12)} ${v}\n`);
process.stdout.write(
  '\nProbes for Claude: tail the newest file in `logs/`, or run Settings → ' +
    'Export diagnostics and hand over the path it prints.\n\n',
);

if (noLaunch) process.exit(0);

process.stdout.write(`launching ${exe} …\n`);
const child = spawn(exe, [], { cwd: repo, detached: true, stdio: 'ignore' });
child.unref();

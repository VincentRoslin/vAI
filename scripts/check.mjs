#!/usr/bin/env node
/**
 * The full check suite (`DEVELOPMENT.md` §4). Runs every gate in order and stops
 * on the first failure. CI and the pre-commit hook call this.
 */
import { execSync } from 'node:child_process';
import process from 'node:process';

const steps = [
  ['rust format', 'cargo fmt --manifest-path src-tauri/Cargo.toml -- --check'],
  ['rust clippy', 'cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings'],
  ['rust test', 'cargo test --manifest-path src-tauri/Cargo.toml'],
  // The rust test above regenerates src/bindings/ via ts-rs — fail on any drift.
  ['ipc bindings in sync', 'git diff --exit-code -- src/bindings'],
  ['frontend format', 'npx prettier --check .'],
  ['frontend lint', 'npx eslint .'],
  ['frontend types', 'npx tsc --noEmit'],
  ['frontend test', 'npx vitest run'],
  ['frontend build', 'npx vite build'],
];

let failed = 0;
for (const [name, cmd] of steps) {
  process.stdout.write(`\n▶ ${name}\n  $ ${cmd}\n`);
  try {
    execSync(cmd, { stdio: 'inherit' });
    process.stdout.write(`  ✓ ${name}\n`);
  } catch {
    process.stdout.write(`  ✗ ${name} FAILED\n`);
    failed += 1;
    break;
  }
}

if (failed) {
  process.stdout.write('\ncheck suite: FAILED\n');
  process.exit(1);
}
process.stdout.write('\ncheck suite: all green\n');

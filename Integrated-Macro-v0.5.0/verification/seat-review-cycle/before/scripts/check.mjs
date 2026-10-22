import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { changedSources, evaluateBuildGate } from './build-gate.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const checks = [];
function run(name, command, args, { required = true } = {}) {
  const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
  const output = [result.stdout, result.stderr, result.error?.message].filter(Boolean).join('\n').trim();
  const status = result.error?.code === 'ENOENT' ? 'blocked' : result.status === 0 ? 'passed' : 'failed';
  checks.push({ name, command: [command, ...args], status, required, exitCode: result.status, output });
  process.stdout.write(`${status.toUpperCase()} ${name}\n${output ? `${output}\n` : ''}`);
  return status;
}
function files(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    if (['target', 'node_modules', 'verification', '.git', '.build-tools'].includes(entry.name)) return [];
    const path = join(directory, entry.name);
    return entry.isDirectory() ? files(path) : [path];
  });
}
const sourceFiles = files(root);
function sourceHashes() {
  return Object.fromEntries(files(root).filter((path) => /\.(rs|swift|js|mjs|toml|json|html|css|h|c|sh|ps1|plist|lock|png|icns|ico|mask)$/.test(path))
    .map((path) => [relative(root, path), createHash('sha256').update(readFileSync(path)).digest('hex')]));
}
const initialFingerprints = sourceHashes();
const testFiles = sourceFiles.filter((path) => path.endsWith('.test.js')).map((path) => relative(root, path));
run('JavaScript behavior tests', process.execPath, ['--test', ...testFiles]);
for (const path of sourceFiles.filter((file) => /\.(js|mjs)$/.test(file) && !file.endsWith('.test.js'))) {
  run(`JavaScript syntax: ${relative(root, path)}`, process.execPath, ['--check', path]);
}
const cargo = run('Rust toolchain', 'cargo', ['--version']);
if (cargo === 'passed') run('Rust core, platform and desktop tests', 'cargo', ['test', '--workspace', '--offline', '--locked']);
else checks.push({ name: 'Rust core, platform and desktop tests', status: 'blocked', required: true, reason: 'Cargo is not available; tests were not run.' });
if (process.platform === 'darwin' && process.argv.includes('--native')) {
  run('macOS native ABI verification', 'bash', ['crates/macro-platform/native/verify-macos.sh']);
}
let bundleStatus = 'not-run';
if (process.platform === 'darwin' && process.argv.includes('--build')) {
  const gate = evaluateBuildGate(checks, initialFingerprints, sourceHashes());
  checks.push({
    name: 'Source stability before packaging', status: gate.changedSources.length ? 'failed' : 'passed',
    required: true, changedSources: gate.changedSources,
    note: 'Packaging can only use the unchanged source that passed the preceding checks.',
  });
  process.stdout.write(`${gate.changedSources.length ? 'FAILED' : 'PASSED'} Source stability before packaging${gate.changedSources.length ? `: ${gate.changedSources.join(', ')}` : ''}\n`);
  if (!gate.allowed) {
    bundleStatus = 'blocked';
    const reason = 'Packaging was not started because required checks did not pass or source changed during verification.';
    checks.push({
      name: 'macOS packaged application', status: 'blocked', required: true,
      reason, blockedBy: gate.blockedBy, changedSources: gate.changedSources,
    });
    process.stdout.write(`BLOCKED macOS packaged application\n${reason}\n`);
  } else {
    bundleStatus = run('macOS packaged application', 'npm',
      ['run', 'build', '--', '--bundles', 'app', '--ci', '--', '--offline', '--locked']);
  }
  if (bundleStatus === 'passed') {
    bundleStatus = run('macOS application signature', 'codesign',
      ['--verify', '--deep', '--strict', 'target/release/bundle/macos/Integrated Macro.app']);
  }
}
const fingerprints = sourceHashes();
const changedSourcePaths = changedSources(initialFingerprints, fingerprints);
checks.push({
  name: 'Source stability during verification', status: changedSourcePaths.length ? 'failed' : 'passed',
  required: true, changedSources: changedSourcePaths,
  note: 'Changes during checks require a fresh run so fingerprints describe the source actually verified.',
});
process.stdout.write(`${changedSourcePaths.length ? 'FAILED' : 'PASSED'} Source stability during verification${changedSourcePaths.length ? `: ${changedSourcePaths.join(', ')}` : ''}\n`);
if (changedSourcePaths.length && bundleStatus === 'passed') bundleStatus = 'invalidated';
const report = {
  generatedAt: new Date().toISOString(),
  environment: { platform: process.platform, arch: process.arch, node: process.version },
  releaseReady: false,
  note: 'Passing unit/ABI checks does not verify actual browser capture, input, overlay, or live booking.',
  checks,
  unverified: [...(bundleStatus === 'passed' ? [] : ['Tauri packaged app']), 'Actual macOS input/capture/overlay', 'Actual Windows input/capture/overlay', 'Authenticated NOL booking flow', 'Browser transport and site contracts', 'End-to-end latency'],
  sourceSha256: fingerprints,
};
mkdirSync(join(root, 'verification'), { recursive: true });
writeFileSync(join(root, 'verification', 'local-checks.json'), `${JSON.stringify(report, null, 2)}\n`);
process.exitCode = checks.some((check) => check.required && check.status === 'failed') ? 1
  : checks.some((check) => check.required && check.status === 'blocked') ? 2 : 0;


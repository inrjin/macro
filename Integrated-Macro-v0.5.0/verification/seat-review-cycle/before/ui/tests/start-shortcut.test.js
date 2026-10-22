import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

for (const mode of ['normal', 'cancellation', 'seats']) {
  const shortcut = { normal: 'F1', cancellation: 'F2', seats: 'F5' }[mode];
  const scenarios = mode === 'cancellation' ? ['current-options', 'normal-validation-bypass']
    : ['current-options', 'validation', 'save-failure', 'cancelled', 'duplicate',
      'busy', 'bootstrap-pending', 'bootstrap-failed', 'overlay'];
  if (mode === 'seats') scenarios.push('normal-validation-bypass');
  if (mode === 'normal') scenarios.push('nol-only-entry');
  for (const scenario of scenarios) {
    test(`${shortcut} ${mode} shortcut handshake: ${scenario}`, () => {
      const result = spawnSync(process.execPath, [
        '--experimental-vm-modules', fileURLToPath(new URL('./run-start-shortcut-dom.mjs', import.meta.url)), scenario, mode,
      ], { encoding: 'utf8', timeout: 15000 });
      assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}\n${result.error || ''}`);
      assert.match(result.stdout, /PASS: F[125] (?:normal|cancellation|seats) shortcut/);
    });
  }
}

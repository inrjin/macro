import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

for (const scenario of ['running-button', 'button-double', 'same-mode', 'different-mode',
  'old-finally', 'old-state-event', 'old-error', 'stop-two-pending', 'invalid-latest',
  'native-cleanup-before-new-success', 'native-stop-old-success', 'native-stop-old-error',
  'old-state-control', 'same-revision-terminal', 'old-state-overlay', 'button-mode-switch', 'cancellation-switch', 'invalid-shortcut-native-newer', 'overlay-bootstrap-error']) {
  test(`replacement execution adversarial DOM: ${scenario}`, () => {
    const result = spawnSync(process.execPath, ['--experimental-vm-modules',
      fileURLToPath(new URL('./run-reentrant-review-dom.mjs', import.meta.url)), scenario],
    { encoding: 'utf8', timeout: 15000 });
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}\n${result.error || ''}`);
    assert.match(result.stdout, /PASS: reentrant review/);
  });
}

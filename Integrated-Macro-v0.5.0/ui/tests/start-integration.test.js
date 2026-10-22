import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

for (const [scenario, description] of [
  ['seats', 'explicit direct-seat mode starts with the common seat range'],
  ['seats-stale-settings', 'direct-seat mode bypasses unrelated schedule and entry prerequisites while retaining settings'],
  ['seats-missing-roi', 'direct-seat mode requires both seat-range corners before saving or starting'],
  ['normal', 'ordinary start sends configuration and start in one request without hidden-screen coordinates'],
  ['scheduled', 'scheduled start preserves sale time and entry target without future calendar coordinates'],
  ['cancellation', 'cancellation start preserves the selected run mode'],
  ['cancellation-stale-settings', 'cancellation bypasses normal-only entry, schedule and clock prerequisites'],
  ['normal-captcha-entry', 'captcha Enter mode starts without an unknown entry button'],
  ['normal-captcha-coordinates', 'captcha Enter mode preserves registered date, session and next clicks'],
  ['normal-nol-coordinates', 'NOL-only captcha mode preserves registered entry coordinates and independent Enter OFF'],
  ['normal-manual-entry', 'manual entry without CAPTCHA detection preserves saved coordinates and bypasses automatic-entry validation'],
  ['normal-manual-nol', 'manual entry and NOL detection are independent options with registered coordinates retained'],
  ['start-failure', 'failed native start stays idle and shows the activation error'],
  ['save-failure', 'failed persistence prevents native start'],
  ['missing-entry', 'scheduled start requires the currently visible entry button'],
]) {
  test(`production UI ${description}`, () => {
    const result = spawnSync(process.execPath, [
      '--experimental-vm-modules', fileURLToPath(new URL('./run-start-dom.mjs', import.meta.url)), scenario,
    ], { encoding: 'utf8', timeout: 15000 });
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}\n${result.error || ''}`);
    assert.ok(result.stdout.includes(`PASS: Production start listener ${scenario}.`));
  });
}

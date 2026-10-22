import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { reconcileProfile, sameProfile } from '../profile-sync.js';

test('new native coordinates merge with independent local settings and retain unknown fields', () => {
  const base = { name: 'saved', vision: { roi: null }, desktop: { future: { enabled: true } } };
  const draft = structuredClone(base); draft.name = 'unsaved';
  const current = structuredClone(base); current.vision.roi = { x: 1, y: 2, width: 3, height: 4 };
  const result = reconcileProfile(base, draft, current);
  assert.equal(result.profile.name, 'unsaved');
  assert.deepEqual(result.profile.vision, current.vision);
  assert.deepEqual(result.profile.desktop, base.desktop);
  assert.deepEqual(result.conflicts, []);
  assert.equal(base.vision.roi, null);
});

test('explicit local clear survives an unrelated remote update but cannot replace a newer capture', () => {
  const base = { name: 'saved', target: { x: 1, y: 2 } };
  const draft = { ...base, target: null };
  assert.equal(reconcileProfile(base, draft, { ...base, name: 'new' }).profile.target, null);
  const current = { ...base, target: { x: 9, y: 8 } };
  const result = reconcileProfile(base, draft, current);
  assert.deepEqual(result.profile.target, current.target);
  assert.deepEqual(result.conflicts, ['target']);
});

test('zone array settings reconcile independently and accepted saves become clean', () => {
  const base = { regions: [{ color: 'old', roi: null }, { color: null, roi: null }] };
  const draft = structuredClone(base); draft.regions[0].color = 'new';
  const current = structuredClone(base); current.regions[0].roi = { x: 1 };
  const result = reconcileProfile(base, draft, current);
  assert.deepEqual(result.profile.regions[0], { color: 'new', roi: { x: 1 } });
  assert.equal(sameProfile(result.profile, current), false);
  assert.equal(sameProfile(reconcileProfile(base, draft, draft).profile, draft), true);
});

test('native color, point, ROI and frame calibration remain whole values across concurrent edits', () => {
  for (const [field, before, local, remote] of [
    ['color', { r: 255, g: 0, b: 0 }, { r: 0, g: 255, b: 0 }, { r: 0, g: 0, b: 255 }],
    ['stage_center', { x: 10, y: 20 }, { x: 30, y: 20 }, { x: 10, y: 40 }],
    ['roi', { x: 10, y: 20, width: 30, height: 40 }, { x: 50, y: 20, width: 30, height: 40 }, { x: 10, y: 20, width: 60, height: 40 }],
    ['capture_reference', { width: 100, height: 200, scale_x: 1, scale_y: 1 }, { width: 300, height: 200, scale_x: 1, scale_y: 1 }, { width: 100, height: 200, scale_x: 2, scale_y: 2 }],
  ]) {
    const result = reconcileProfile({ [field]: before, tolerance: 12 }, { [field]: local, tolerance: 15 }, { [field]: remote, tolerance: 12 });
    assert.deepEqual(result.profile[field], remote, `${field} must never combine sampled components`);
    assert.equal(result.profile.tolerance, 15, 'Independent options must still merge');
    assert.deepEqual(result.conflicts, [field]);
  }
});

test('production UI preserves capture revisions and sends one cancellable start request', () => {
  const result = spawnSync(process.execPath, ['--experimental-vm-modules', fileURLToPath(new URL('./run-profile-race-dom.mjs', import.meta.url))], { encoding: 'utf8', timeout: 15000 });
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  assert.match(result.stdout, /PASS: profile races/);
});

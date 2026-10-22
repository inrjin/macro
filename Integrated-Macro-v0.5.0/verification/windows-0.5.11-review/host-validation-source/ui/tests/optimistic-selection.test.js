import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { defaultProfile, normalizeSnapshot, validateStart } from '../model.js';

test('direct seats display the unique delivered estimate without changing confirmed count', () => {
  const snapshot = normalizeSnapshot({
    mode: 'seats', selectionPolicy: 'optimistic', phase: 'seats',
    selected_ids: ['pixel:10:10'], attempted_ids: ['pixel:10:10', 'pixel:20:20'],
    current_candidate_id: 'pixel:30:30', target_seats: 2,
  });
  assert.equal(snapshot.selectedCount, 1);
  assert.equal(snapshot.estimatedCount, 2);
  assert.equal(snapshot.displaySelectedCount, 2);
  assert.equal(snapshot.selectionCountLabel, '선택 추정');
  assert.equal(snapshot.selectedIds.includes('pixel:30:30'), false, 'Queued clicks are not delivered estimates');
});

test('runtime estimate is authoritative when raw history includes failed earlier attempts', () => {
  const snapshot = normalizeSnapshot({
    mode: 'seats', selectionPolicy: 'optimistic', estimatedCount: 1, selectedCount: 0,
    selected_ids: [], attempted_ids: ['pixel:10:10', 'pixel:20:20'],
  });
  assert.equal(snapshot.displaySelectedCount, 1);
  assert.equal(snapshot.selectedCount, 0);
  assert.equal(snapshot.selectionCountLabel, '선택 추정');
});

test('explicit observed policy continues displaying confirmed selections', () => {
  for (const mode of ['normal', 'cancellation']) {
    const snapshot = normalizeSnapshot({
      mode, selectionPolicy: 'observed', estimatedCount: 2,
      selected_ids: ['a'], attempted_ids: ['a', 'b'],
    });
    assert.equal(snapshot.selectedCount, 1);
    assert.equal(snapshot.displaySelectedCount, 1);
    assert.equal(snapshot.selectionPolicy, 'confirmed');
    assert.equal(snapshot.selectionCountLabel, '확인된 선택');
  }
});

test('runtime optimistic policy applies to normal and cancellation flows too', () => {
  for (const mode of ['seats', 'normal', 'cancellation']) {
    const snapshot = normalizeSnapshot({
      mode, selectionPolicy: 'optimistic', estimatedCount: 2,
      selected_ids: ['a'], attempted_ids: ['a', 'b'], current_candidate_id: 'queued',
    });
    assert.equal(snapshot.selectedCount, 1);
    assert.equal(snapshot.displaySelectedCount, 2);
    assert.equal(snapshot.selectionCountLabel, '선택 추정');
  }
});

test('direct seats never require a selected-seat color', () => {
  const profile = defaultProfile();
  profile.vision.roi = { x: 0, y: 0, width: 100, height: 100 };
  profile.desktop.selected_color = null;
  assert.doesNotThrow(() => validateStart(profile, 'seats'));
});

for (const mode of ['seats', 'normal', 'cancellation']) {
  test(`production ${mode} optimistic controls retain one-click reporting during automatic recovery`, () => {
    const result = spawnSync(process.execPath, [
      '--experimental-vm-modules', fileURLToPath(new URL('./run-optimistic-selection-dom.mjs', import.meta.url)), mode,
    ], { encoding: 'utf8', timeout: 15000 });
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}\n${result.error || ''}`);
    assert.match(result.stdout, /PASS: Production optimistic selection/);
  });
}

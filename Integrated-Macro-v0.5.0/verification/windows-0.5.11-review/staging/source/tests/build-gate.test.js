import assert from 'node:assert/strict';
import test from 'node:test';
import { changedSources, evaluateBuildGate } from '../scripts/build-gate.mjs';

const verifiedSource = { 'src/engine.rs': 'engine-v1', 'ui/app.js': 'ui-v1' };
const passedChecks = [
  { name: 'JavaScript behavior tests', required: true, status: 'passed' },
  { name: 'Rust workspace tests', required: true, status: 'passed' },
];

test('packaging requires all required checks to pass, including unavailable or unfinished checks', () => {
  for (const status of ['failed', 'blocked', 'not-run', 'running', undefined]) {
    const checks = [
      ...passedChecks,
      { name: 'Native ABI verification', required: true, status },
    ];
    const gate = evaluateBuildGate(checks, verifiedSource, verifiedSource);
    assert.equal(gate.allowed, false, `must block ${status}`);
    assert.deepEqual(gate.blockedBy, [{ name: 'Native ABI verification', status }]);
    assert.deepEqual(gate.changedSources, []);
  }
});

test('changing, adding or deleting source after successful tests blocks packaging', () => {
  for (const current of [
    { ...verifiedSource, 'src/engine.rs': 'engine-v2' },
    { ...verifiedSource, 'src/new.rs': 'new-source' },
    { 'src/engine.rs': 'engine-v1' },
  ]) {
    const gate = evaluateBuildGate(passedChecks, verifiedSource, current);
    assert.equal(gate.allowed, false);
    assert.equal(gate.changedSources.length, 1);
    assert.deepEqual(gate.blockedBy, []);
  }
});

test('passing required checks and unchanged source allow packaging despite an optional failure', () => {
  const gate = evaluateBuildGate([
    ...passedChecks,
    { name: 'Optional diagnostic', required: false, status: 'failed' },
  ], verifiedSource, { 'ui/app.js': 'ui-v1', 'src/engine.rs': 'engine-v1' });
  assert.deepEqual(gate, { allowed: true, blockedBy: [], changedSources: [] });
});

test('the gate reports every blocker and final source comparison detects changes during packaging', () => {
  const failedChecks = [
    { name: 'JavaScript behavior tests', required: true, status: 'failed' },
    { name: 'Rust workspace tests', required: true, status: 'blocked' },
  ];
  const editedSource = { 'src/engine.rs': 'engine-v2', 'ui/new.js': 'new-ui' };
  const beforeChecks = structuredClone(failedChecks);
  const beforeSource = structuredClone(verifiedSource);
  const gate = evaluateBuildGate(failedChecks, verifiedSource, editedSource);
  assert.equal(gate.allowed, false);
  assert.equal(gate.blockedBy.length, 2);
  assert.deepEqual(gate.changedSources, ['src/engine.rs', 'ui/app.js', 'ui/new.js']);
  assert.deepEqual(changedSources(verifiedSource, editedSource), gate.changedSources);
  assert.deepEqual(failedChecks, beforeChecks);
  assert.deepEqual(verifiedSource, beforeSource);
});

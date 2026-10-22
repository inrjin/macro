// Adversarial listener-level checks: real app module, simulated Tauri replies.
// This fixture does not execute Rust, OS input, browser layout, or ticket sites.
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
import { defaultProfile } from '../model.js';
import { loadAppPage } from './dom-fixture.mjs';

const scenario = process.argv[2];
const profile = defaultProfile();
profile.vision.roi = { x: 20, y: 30, width: 400, height: 300 };
const initial = { phase: 'idle', mode: 'normal', running: false, paused: false,
  runId: 0, revision: 0, profileRevision: 0, captureState: 'idle',
  manualActions: [], checks: [], selectedCount: 0, targetCount: 2 };
const state = (runId, mode, message, phase = mode === 'normal' ? 'entry' : 'seats') => ({
  ...initial, phase, mode, running: !['idle', 'stopped'].includes(phase),
  runId, revision: 1, profileRevision: runId, message,
});
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const starts = [];
const stop = deferred();
const pageName = ['old-state-overlay', 'overlay-bootstrap-error'].includes(scenario) ? 'overlay.html' : 'index.html';
const bootstrapGate = deferred();
const pageURL = process.env.REENTRANT_UI_ROOT
  ? pathToFileURL(`${process.env.REENTRANT_UI_ROOT}/${pageName}`)
  : new URL(`../${pageName}`, import.meta.url);
const page = await loadAppPage(pageURL, initial, async (name, args) => {
  if (name === 'stop_run') return stop.promise;
  assert.ok(['start_shortcut', 'start_run'].includes(name), `Unexpected command ${name}`);
  const pending = deferred(); starts.push({ name, args, ...pending });
  return pending.promise;
}, profile, scenario === 'overlay-bootstrap-error' ? { bootstrapWait: bootstrapGate.promise } : {});
const $ = (selector) => page.document.querySelector(selector);
const request = (id, mode) => page.listeners['shortcut-start-requested'][0]({
  payload: { requestId: id, mode, shortcut: { normal: 'F1', cancellation: 'F2', seats: 'F5' }[mode] },
});
const reply = (index, runId, mode, message, cancelled = false) => {
  const payload = { profile: starts[index].args.profile || profile, profileRevision: runId,
    snapshot: state(runId, mode, message), controlEpoch: runId };
  if (cancelled) payload.cancelled = true;
  starts[index].resolve(payload);
};
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

if (scenario === 'overlay-bootstrap-error') {
  page.emit('state-change', { ...state(2, 'seats', 'LIVE_OVERLAY'), controlEpoch: 20 });
  bootstrapGate.reject(new Error('OLD_BOOTSTRAP_ERROR'));
  await flush();
  assert.equal($('#overlay-message').textContent, 'LIVE_OVERLAY',
    'Live state must not be replaced by an earlier bootstrap failure');
} else if (scenario === 'invalid-shortcut-native-newer') {
  $('[data-setting="features.scheduledStart"]').checked = true;
  const starting = page.listeners['shortcut-start-requested'][0]({
    payload: { requestId: 1, mode: 'normal', shortcut: 'F1', controlEpoch: 11 },
  });
  assert.equal(starts.length, 1);
  assert.equal(starts[0].args.profile, null);
  assert.match(starts[0].args.error, /판매 시작 시각/);
  // The newer F9 is routed entirely in Rust, so it emits state without a
  // shortcut-start-requested event or a local responseEpoch increment.
  page.emit('state-change', { ...state(1, 'normal', 'LATEST_NATIVE_F9', 'seats'),
    revision: 2, controlEpoch: 12 });
  starts[0].reject(new Error('OLD_INVALID_SHORTCUT_ERROR')); await starting;
  assert.equal($('#phase-message').textContent, 'LATEST_NATIVE_F9');
  assert.equal($('#feedback').textContent.includes('OLD_INVALID_SHORTCUT_ERROR'), false,
    'An old validation-rejection reply must not put feedback over a newer native F9');
} else if (scenario === 'old-state-overlay') {
  page.emit('state-change', { ...state(2, 'seats', 'LATEST_OVERLAY'), controlEpoch: 20 });
  page.emit('state-change', { ...state(1, 'normal', 'OLD_OVERLAY'), revision: 999, controlEpoch: 19 });
  assert.equal($('#overlay-message').textContent, 'LATEST_OVERLAY');
} else if (scenario === 'button-mode-switch') {
  const first = $('#start-button').click();
  // The minimal selector parser has one attribute; find the radio explicitly.
  const radios = page.document.querySelectorAll('input[name="run-mode"]');
  const cancellation = radios.find((field) => field.value === 'cancellation');
  assert.equal(cancellation.disabled, false, 'Mode choice must remain usable during an execution request');
  for (const field of radios) field.checked = field === cancellation;
  for (const listener of cancellation.listeners.change || []) listener({ target: cancellation });
  const second = $('#start-button').click();
  assert.equal(starts.length, 2);
  assert.equal(starts[0].args.mode, 'normal');
  assert.equal(starts[1].args.mode, 'cancellation');
  reply(1, 2, 'cancellation', 'LATEST_CANCELLATION'); await second;
  reply(0, 1, 'normal', 'OLD_NORMAL'); await first;
  assert.equal($('#phase-message').textContent, 'LATEST_CANCELLATION');
} else if (scenario === 'native-cleanup-before-new-success') {
  page.emit('state-change', { ...state(1, 'normal', 'OLD_RUN'), controlEpoch: 10 });
  const starting = $('#start-button').click();
  assert.equal(starts.length, 1);
  // The new command is still preparing; its ordered cleanup can be observed
  // before the new run has a runId. It must not cancel that same new start.
  page.emit('state-change', { ...state(1, 'normal', 'CLEANUP', 'stopped'), controlEpoch: 11 });
  starts[0].resolve({ profile, profileRevision: 2, controlEpoch: 12,
    snapshot: { ...state(2, 'normal', 'NEW_RUN'), controlEpoch: 12 } });
  await starting;
  assert.equal($('#phase-message').textContent, 'NEW_RUN');
} else if (scenario === 'native-stop-old-success' || scenario === 'native-stop-old-error') {
  page.emit('state-change', { ...state(1, 'normal', 'OLD_RUN'), controlEpoch: 10 });
  const starting = request(1, 'seats');
  page.emit('state-change', { ...state(1, 'normal', 'NATIVE_STOP', 'stopped'), controlEpoch: 12 });
  if (scenario.endsWith('error')) starts[0].reject(new Error('SUPERSEDED_NATIVE_ERROR'));
  else starts[0].resolve({ profile, profileRevision: 2, controlEpoch: 11,
    snapshot: { ...state(2, 'seats', 'SUPERSEDED_SUCCESS'), controlEpoch: 11 } });
  await starting;
  assert.equal($('#phase-message').textContent, 'NATIVE_STOP');
  assert.equal($('#status-symbol').classList.contains('running'), false);
  assert.equal($('#feedback').textContent.includes('SUPERSEDED_NATIVE_ERROR'), false);
} else if (scenario === 'old-state-control') {
  page.emit('state-change', { ...state(2, 'seats', 'NATIVE_STOP', 'stopped'), controlEpoch: 20 });
  page.emit('state-change', { ...state(2, 'seats', 'OLD_SAME_RUN'), revision: 999, controlEpoch: 19 });
  assert.equal($('#phase-message').textContent, 'NATIVE_STOP');
} else if (scenario === 'same-revision-terminal') {
  page.emit('state-change', { ...state(2, 'seats', 'NATIVE_STOP', 'stopped'), controlEpoch: 20 });
  page.emit('state-change', { ...state(2, 'seats', 'OLD_SAME_REVISION'), controlEpoch: 20 });
  assert.equal($('#phase-message').textContent, 'NATIVE_STOP');
} else if (scenario === 'running-button') {
  page.emit('state-change', state(1, 'normal', 'RUN_1'));
  assert.equal($('#start-button').disabled, false, 'Running must not disable the same Start button');
  const second = $('#start-button').click();
  assert.equal(starts.length, 1, 'Running Start button must transmit a replacement request');
  assert.equal(starts[0].name, 'start_run');
  reply(0, 2, 'normal', 'RUN_2'); await second;
  assert.equal($('#phase-message').textContent, 'RUN_2');
} else if (scenario === 'button-double') {
  const first = $('#start-button').click();
  assert.equal($('#start-button').disabled, false, 'Pending execution must permit a replacement Start');
  const second = $('#start-button').click();
  assert.equal(starts.length, 2, 'Both distinct button activations must transmit');
  reply(1, 2, 'normal', 'LATEST_BUTTON'); await second;
  reply(0, 1, 'normal', 'OLD_BUTTON'); await first;
  assert.equal($('#phase-message').textContent, 'LATEST_BUTTON');
} else if (scenario === 'old-state-event') {
  page.emit('state-change', state(2, 'seats', 'LATEST_EVENT'));
  page.emit('state-change', { ...state(1, 'normal', 'OLD_EVENT'), revision: 999 });
  assert.equal($('#phase-message').textContent, 'LATEST_EVENT', 'Lower runId cannot replace a newer run even with a larger per-run revision');
} else {
  const first = request(1, 'normal');
  const secondMode = scenario === 'same-mode' ? 'normal' : scenario === 'cancellation-switch' ? 'cancellation' : 'seats';
  const second = request(2, secondMode);
  assert.equal(starts.length, 2, 'A later native request must reach IPC while the first reply is pending');
  assert.equal(starts[1].args.error, null, 'Busy execution is not a rejection reason for a replacement start');
  assert.ok(starts[1].args.profile, 'The newest request must receive current options');
  await request(2, secondMode);
  assert.equal(starts.length, 2, 'An identical requestId is a replay, not another user command');
  if (scenario === 'old-finally') {
    reply(0, 1, 'normal', 'OLD_CANCELLED', true); await first;
    assert.equal($('#save-button').disabled, true, 'An older finally cannot release the newest execution lock');
    assert.equal($('#start-button').disabled, false, 'A third execution remains available');
    reply(1, 2, secondMode, 'LATEST_PENDING'); await second;
    assert.equal($('#phase-message').textContent, 'LATEST_PENDING');
  } else if (scenario === 'stop-two-pending') {
    const stopping = $('#stop-button').click();
    assert.equal($('#start-button').disabled, true, 'Start waits while a Stop response is pending');
    stop.resolve({ ...state(2, secondMode, 'STOPPED', 'stopped'), profileRevision: 0 }); await stopping;
    reply(1, 2, secondMode, 'LATE_SECOND_SUCCESS'); await second;
    reply(0, 1, 'normal', 'LATE_FIRST_SUCCESS'); await first;
    assert.equal($('#phase-message').textContent, 'STOPPED');
    assert.equal($('#status-symbol').classList.contains('running'), false);
  } else if (scenario === 'old-error') {
    reply(1, 2, secondMode, 'LATEST_SUCCESS'); await second;
    const bootstrapCalls = page.calls.filter(({ name }) => name === 'get_bootstrap').length;
    starts[0].reject(new Error('OLD_ERROR_MUST_NOT_REPLACE_NEW_RUN')); await first;
    assert.equal($('#phase-message').textContent, 'LATEST_SUCCESS');
    assert.equal($('#feedback').textContent.includes('OLD_ERROR_MUST_NOT_REPLACE_NEW_RUN'), false);
    assert.equal(page.calls.filter(({ name }) => name === 'get_bootstrap').length, bootstrapCalls,
      'A superseded failure must not fetch and render an unrelated bootstrap');
  } else if (scenario === 'invalid-latest') {
    page.emit('state-change', { ...state(2, secondMode, 'LATEST_REJECTED', 'stopped'), profileRevision: 0 });
    starts[1].reject(new Error('LATEST_INVALID_OPTIONS')); await second;
    reply(0, 1, 'normal', 'OLD_SUCCESS'); await first;
    assert.equal($('#status-symbol').classList.contains('running'), false, 'Invalid latest request cannot revive the old run');
  } else {
    assert.ok(['same-mode', 'different-mode', 'cancellation-switch'].includes(scenario));
    reply(1, 2, secondMode, 'LATEST_SUCCESS'); await second;
    reply(0, 1, 'normal', 'OLD_SUCCESS'); await first;
    assert.equal($('#phase-message').textContent, 'LATEST_SUCCESS');
    assert.equal($('#mode-pill').textContent, { normal: '일반 예매', cancellation: '취소표 탐색', seats: '좌석 바로 탐색' }[secondMode]);
  }
}
await flush();
console.log(`PASS: reentrant review ${scenario}`);
console.log('Scope: real UI event handlers with deferred IPC; Rust scheduling and native input are not simulated.');

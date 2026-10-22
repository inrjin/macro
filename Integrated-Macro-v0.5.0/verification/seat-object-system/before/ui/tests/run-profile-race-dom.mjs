import assert from 'node:assert/strict';
import { defaultProfile, applyCapture } from '../model.js';
import { loadAppPage } from './dom-fixture.mjs';

const html = new URL('../index.html', import.meta.url);
const clone = value => JSON.parse(JSON.stringify(value));
const initial = { phase: 'idle', runId: 0, revision: 0, mode: 'normal', running: false, paused: false,
  captureState: 'idle', captureField: '', captureArmed: false, captureError: null, message: '', checks: [], manualActions: [], selectedCount: 0, targetCount: 2 };
const frame = { width: 1200, height: 800, scale_x: 1, scale_y: 1 };
const color = { r: 130, g: 70, b: 210 };
const start = { field: 'vision.roi.start', point: { x: 100, y: 120 }, color, frame };
const end = { field: 'vision.roi.end', point: { x: 700, y: 620 }, color, frame };
const before = applyCapture(defaultProfile(), start);
const captured = applyCapture(before, end);
const q = (page, selector) => page.document.querySelector(selector);
const changed = (page, setting, value) => {
  for (const el of page.document.querySelectorAll(`[data-setting="${setting}"]`)) {
    if (el.type === 'checkbox') el.checked = value; else el.value = String(value);
    for (const listener of el.listeners.input || []) listener({ target: el });
  }
};

// The reading-state guard closes the observed UI window; native CAS protects
// dispatches already in flight before the reading event reaches the WebView.
{
  let current = clone(before), revision = 0;
  const page = await loadAppPage(html, initial, async (name, args) => {
    assert.equal(args.expectedProfileRevision, revision);
    current = clone(args.profile); revision += 1;
    if (name === 'arm_capture') return { field: args.field, profile: current, profileRevision: revision,
      snapshot: { ...initial, revision: 1, captureField: args.field, captureState: 'armed', captureArmed: true } };
    assert.equal(name, 'save_profile');
    return { profile: current, profileRevision: revision };
  }, current);
  await q(page, '[data-capture-field="vision.roi.end"]').click();
  page.emit('state-change', { ...initial, revision: 2, captureField: end.field, captureState: 'reading', captureArmed: true });
  assert.equal(q(page, '#save-button').disabled, true);
  assert.equal(q(page, '[data-capture-field="vision.roi.start"]').disabled, true);
  const count = page.calls.length;
  await q(page, '#save-button').click();
  assert.equal(page.calls.length, count);
  current = applyCapture(current, end); revision += 1;
  page.emit('point-captured', { ...end, profile: current, profileRevision: revision });
  page.emit('state-change', { ...initial, revision: 3, captureState: 'captured', captureField: end.field, captureArmed: false });
  await q(page, '#save-button').click();
  assert.deepEqual(clone(page.calls.at(-1).args.profile.vision.roi), captured.vision.roi);
  assert.equal(page.calls.at(-1).args.expectedProfileRevision, 2);
}

// A snapshot advertises a new revision before its point event. It must not
// authorize an old full profile. A CAS conflict reloads native coordinates and
// preserves independent unsaved options, without automatically starting.
{
  let current = clone(before), revision = 0, rejectSave;
  let rejecting = true;
  const page = await loadAppPage(html, initial, async (name, args) => {
    assert.equal(name, 'save_profile');
    if (rejecting) return new Promise((_, reject) => { rejectSave = reject; });
    assert.equal(args.expectedProfileRevision, revision);
    current = clone(args.profile); revision += 1;
    return { profile: current, profileRevision: revision };
  }, current, { bootstrapProvider: state => ({ profile: current, profileRevision: revision, snapshot: state, windows: [], checks: [] }) });
  changed(page, 'targetCount', 3);
  current = applyCapture(current, end); revision = 1;
  page.emit('state-change', { ...initial, revision: 10, profileRevision: 1 });
  const saving = q(page, '#save-button').click();
  assert.equal(page.calls.at(-1).args.expectedProfileRevision, 0, 'A snapshot alone cannot advance the draft revision');
  assert.equal(page.calls.at(-1).args.profile.vision.roi, null);
  rejectSave(new Error('[profile_revision_conflict] 설정이 변경되었습니다.'));
  await saving;
  assert.match(q(page, '[data-point-field="vision.roi.end"]').textContent, /700.*620/);
  assert.equal(q(page, '[data-setting="targetCount"]').value, 3);
  assert.match(q(page, '#save-status').textContent, /저장하지 않은 변경/);
  assert.equal(page.calls.some(call => call.name === 'start_run'), false);
  rejecting = false;
  await q(page, '#save-button').click();
  assert.equal(page.calls.at(-1).args.expectedProfileRevision, 1);
  assert.deepEqual(clone(current.vision.roi), captured.vision.roi);
  assert.equal(current.target_seats, 3);
}

// A Save was processed first but its response arrives after a newer capture.
// The late response must not roll back either the profile or its revision.
{
  let releaseSave, sent;
  let calls = 0;
  const page = await loadAppPage(html, initial, async (name, args) => {
    assert.equal(name, 'save_profile'); sent = clone(args);
    if (++calls === 1) return new Promise(resolve => { releaseSave = () => resolve({ profile: sent.profile, profileRevision: 1 }); });
    return { profile: args.profile, profileRevision: 3 };
  }, before);
  const saving = q(page, '#save-button').click();
  const newer = applyCapture(sent.profile, end);
  page.emit('point-captured', { ...end, profile: newer, profileRevision: 2 });
  releaseSave(); await saving;
  assert.match(q(page, '[data-point-field="vision.roi.end"]').textContent, /700.*620/);
  page.emit('point-captured', { ...start, profile: before, profileRevision: 1 });
  await q(page, '#save-button').click();
  assert.equal(sent.expectedProfileRevision, 2);
  assert.deepEqual(sent.profile.vision.roi, captured.vision.roi);
}

// No Save IPC precedes Start anymore. F4 can cancel this one native transaction
// without an old JS continuation subsequently sending a fresh start request.
{
  let releaseStart;
  const page = await loadAppPage(html, initial, async (name, args) => {
    assert.equal(name, 'start_run');
    assert.equal(args.expectedProfileRevision, 0);
    assert.equal(args.mode, 'normal');
    assert.ok(args.profile);
    return new Promise(resolve => { releaseStart = () => resolve({ cancelled: true }); });
  }, before);
  const starting = q(page, '#start-button').click();
  page.emit('state-change', { ...initial, phase: 'stopped', revision: 4, message: '사용자 중지' });
  releaseStart(); await starting;
  assert.deepEqual(page.calls.filter(call => call.name !== 'get_bootstrap').map(call => call.name), ['start_run']);
  assert.equal(q(page, '#status-symbol').classList.contains('running'), false);
  assert.equal(q(page, '#feedback').hidden, true);
}

// The native arm command minimizes the settings window before its reply. A
// quick F8 can therefore finish first; that older reply must not re-arm it.
{
  let releaseArm;
  const page = await loadAppPage(html, initial, async (name, args) => {
    assert.equal(name, 'arm_capture');
    return new Promise(resolve => { releaseArm = () => resolve({ field: args.field, profile: args.profile, profileRevision: 1,
      snapshot: { ...initial, revision: 1, profileRevision: 1, captureField: end.field, captureState: 'armed', captureArmed: true } }); });
  }, before);
  const arming = q(page, '[data-capture-field="vision.roi.end"]').click();
  page.emit('point-captured', { ...end, profile: captured, profileRevision: 2 });
  releaseArm(); await arming;
  assert.match(q(page, '#capture-guide-title').textContent, /지정 완료/);
  assert.match(q(page, '[data-point-field="vision.roi.end"]').textContent, /700.*620/);
  page.emit('state-change', { ...initial, revision: 2, profileRevision: 1, captureField: end.field, captureState: 'reading', captureArmed: true });
  assert.match(q(page, '#capture-guide-title').textContent, /지정 완료/);
}

// Persistence can succeed before activation fails or F4 cancels the remaining
// start. Both results must leave the next deliberate start on the saved revision.
for (const outcome of ['error', 'cancelled']) {
  let current = clone(before), revision = 0, starts = 0;
  const page = await loadAppPage(html, initial, async (name, args) => {
    assert.equal(name, 'start_run');
    assert.equal(args.expectedProfileRevision, revision);
    current = clone(args.profile); revision += 1; starts += 1;
    if (starts === 1 && outcome === 'error') throw new Error('[target_activation_failed] 전면 전환 실패');
    return { cancelled: true, profile: current, profileRevision: revision,
      snapshot: { ...initial, phase: 'stopped', revision, profileRevision: revision } };
  }, current, { bootstrapProvider: state => ({ profile: current, profileRevision: revision, snapshot: state, windows: [], checks: [] }) });
  changed(page, 'targetCount', 3);
  await q(page, '#start-button').click();
  assert.equal(starts, 1, 'An error/cancellation never causes automatic start retry');
  await q(page, '#start-button').click();
  assert.equal(starts, 2);
  assert.equal(page.calls.filter(call => call.name === 'start_run')[1].args.expectedProfileRevision, 1);
  assert.equal(current.target_seats, 3);
}

// An unsaved color/position is persisted by Arm, but its reply can follow the
// F8 result. Channels and axes belong to one sample, not independent settings.
for (const field of ['color', 'desktop.stage_center']) {
  const base = clone(before);
  base.color = { r: 255, g: 0, b: 0 };
  base.desktop.stage_center = { x: 10, y: 20 };
  let releaseArm, sent;
  const page = await loadAppPage(html, initial, async (name, args) => {
    if (name === 'save_profile') return { profile: args.profile, profileRevision: 3 };
    assert.equal(name, 'arm_capture'); sent = clone(args.profile);
    return new Promise(resolve => { releaseArm = () => resolve({ field, profile: sent, profileRevision: 1,
      snapshot: { ...initial, revision: 1, profileRevision: 1, captureField: field, captureState: 'armed', captureArmed: true } }); });
  }, base);
  if (field === 'color') changed(page, 'seatColor', '#00ff00');
  else {
    const imported = clone(base); imported.desktop.stage_center = { x: 30, y: 20 };
    const file = q(page, '#import-file');
    file.files = [{ size: 100, text: async () => JSON.stringify(imported) }];
    await file.listeners.change[0]({ target: file });
  }
  changed(page, 'targetCount', 3);
  const arming = q(page, `[data-capture-field="${field}"]`).click();
  const sampled = { field, point: { x: 10, y: 40 }, color: { r: 0, g: 0, b: 255 }, frame };
  const current = applyCapture(sent, sampled);
  page.emit('point-captured', { ...sampled, profile: current, profileRevision: 2 });
  releaseArm(); await arming;
  await q(page, '#save-button').click();
  const saved = page.calls.at(-1).args;
  assert.equal(saved.expectedProfileRevision, 2);
  assert.deepEqual(clone(saved.profile.color), current.color);
  assert.deepEqual(clone(saved.profile.desktop.stage_center), current.desktop.stage_center);
  assert.equal(saved.profile.target_seats, 3);
  assert.match(q(page, '#capture-guide-title').textContent, /지정 완료/);
}

// The on-screen Stop must be usable without F4 while Start/Save is awaiting a
// native response. Its own lock neither releases nor steals the earlier lock.
for (const pendingName of ['start_run', 'save_profile']) {
  let releaseOriginal, releaseStop;
  const page = await loadAppPage(html, initial, async (name, args) => {
    if (name === pendingName) return new Promise(resolve => { releaseOriginal = () => resolve({ cancelled: true }); });
    assert.equal(name, 'stop_run');
    return new Promise(resolve => { releaseStop = () => resolve({ ...initial, phase: 'stopped', revision: 4, message: '사용자 중지' }); });
  }, before);
  const pending = q(page, pendingName === 'start_run' ? '#start-button' : '#save-button').click();
  assert.equal(q(page, '#stop-button').disabled, false);
  const stopping = q(page, '#stop-button').click();
  assert.equal(q(page, '#stop-button').disabled, true);
  await q(page, '#stop-button').click();
  assert.equal(page.calls.filter(call => call.name === 'stop_run').length, 1);
  releaseStop(); await stopping;
  assert.equal(q(page, '#start-button').disabled, true, 'Stop cannot unlock an older pending operation');
  assert.equal(q(page, '#save-button').disabled, true);
  releaseOriginal(); await pending;
  assert.equal(q(page, '#start-button').disabled, false);
  assert.equal(page.calls.filter(call => call.name === 'start_run').length, pendingName === 'start_run' ? 1 : 0);
}

// The reverse completion order must also preserve the pending Stop lock.
{
  let releaseStart, releaseStop;
  const page = await loadAppPage(html, initial, async name => {
    if (name === 'start_run') return new Promise(resolve => { releaseStart = () => resolve({ cancelled: true }); });
    assert.equal(name, 'stop_run');
    return new Promise(resolve => { releaseStop = () => resolve({ ...initial, phase: 'stopped', revision: 4 }); });
  }, before);
  const starting = q(page, '#start-button').click();
  const stopping = q(page, '#stop-button').click();
  releaseStart(); await starting;
  assert.equal(q(page, '#start-button').disabled, true);
  assert.equal(q(page, '#save-button').disabled, true);
  assert.equal(q(page, '#check-readiness').disabled, true);
  releaseStop(); await stopping;
  assert.equal(q(page, '#start-button').disabled, false);
}

// Bootstrap cannot be raced by persisting default controls. File reads begun
// before a native capture also cannot replace the newly adopted draft later.
{
  let releaseBootstrap;
  const bootstrapWait = new Promise(resolve => { releaseBootstrap = resolve; });
  const page = await loadAppPage(html, initial, async () => { throw new Error('No mutation expected'); }, before, { bootstrapWait });
  assert.equal(q(page, '#save-button').disabled, true);
  assert.equal(q(page, '#start-button').disabled, true);
  assert.equal(q(page, '[data-capture-field="vision.roi.end"]').disabled, true);
  releaseBootstrap(); await new Promise(resolve => setTimeout(resolve, 0));
  let releaseFile;
  const file = q(page, '#import-file');
  file.files = [{ size: 100, text: () => new Promise(resolve => { releaseFile = resolve; }) }];
  const importing = file.listeners.change[0]({ target: file });
  page.emit('point-captured', { ...end, profile: captured, profileRevision: 1 });
  releaseFile(JSON.stringify(before)); await importing;
  assert.match(q(page, '[data-point-field="vision.roi.end"]').textContent, /700.*620/);
  assert.match(q(page, '#feedback').textContent, /불러오기를 취소/);
}

// The worker can successfully start and then stop within one publish interval.
// Those two replies have identical revisions; a late successful Start reply
// must not reverse either an on-screen Stop or the F4 stopped state event.
for (const stopVia of ['button', 'F4']) {
  for (const startVia of ['button', 'shortcut']) {
    let releaseStart;
    const stopped = { ...initial, runId: 1, revision: 7, profileRevision: 1, phase: 'stopped', message: '사용자 중지' };
    const started = { ...stopped, phase: 'seats', running: true, message: '좌석 탐색' };
    const page = await loadAppPage(html, initial, async (name, args) => {
      if (name === 'stop_run') return stopped;
      assert.equal(name, startVia === 'button' ? 'start_run' : 'start_shortcut');
      return new Promise(resolve => { releaseStart = () => resolve({ profile: args.profile, profileRevision: 1, snapshot: started }); });
    }, before);
    const starting = startVia === 'button' ? q(page, '#start-button').click()
      : page.emit('shortcut-start-requested', { requestId: 1, mode: 'normal' });
    if (stopVia === 'button') await q(page, '#stop-button').click();
    else page.emit('state-change', stopped);
    assert.equal(q(page, '#status-symbol').classList.contains('running'), false);
    releaseStart(); await starting;
    await new Promise(resolve => setTimeout(resolve, 0));
    assert.equal(q(page, '#status-symbol').classList.contains('running'), false, `${startVia}/${stopVia}: old same-revision success cannot replace Stop`);
    assert.match(q(page, '#phase-message').textContent, /사용자 중지/);
    assert.equal(q(page, '#start-button').disabled, false);
  }
}
console.log('PASS: profile races: F8 reading, snapshot-before-profile, CAS reconciliation, late Save/capture, atomic Start/F4, bootstrap and delayed import.');
console.log('Limits: production UI in VM DOM with controlled IPC; no live Tauri, OS input, or user settings.');

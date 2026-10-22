import test from 'node:test';
import assert from 'node:assert/strict';
import {
  FEATURE_OPTIONS, applyCapture, candidatePositionLabel, captureValue, clearCapture, defaultProfile, fromForm, mergeProfile,
  normalizeSnapshot, toForm, toLocalDateTime, validateStart,
} from '../model.js';

test('new features remain independently off and unknown future screens are not required to save', () => {
  const profile = defaultProfile();
  const saved = fromForm(toForm(profile), profile);
  assert.ok(Object.values(saved.features).every((enabled) => enabled === false));
  assert.equal(saved.desktop.scheduled_start, false);
  assert.equal(saved.vision.roi, null);
  assert.equal(saved.legacy.complete, null);
  assert.doesNotThrow(() => validateStart(saved));
});

test('NOL captcha detection remains an independent default-off option across old profiles and form saves', () => {
  const previous = mergeProfile({ features: { auto_captcha_resume: true } });
  assert.equal(previous.features.nol_captcha_detection, false, 'Older settings cannot silently enable NOL detection');
  assert.equal(FEATURE_OPTIONS.find(([key]) => key === 'nolCaptchaDetection')[1], 'NOL 캡챠 판단');
  for (const nolCaptchaDetection of [false, true]) {
    for (const captchaResume of [false, true]) {
      const form = toForm(previous);
      form.features.nolCaptchaDetection = nolCaptchaDetection;
      form.features.captchaResume = captchaResume;
      const saved = fromForm(form, previous);
      assert.equal(saved.features.nol_captcha_detection, nolCaptchaDetection);
      assert.equal(saved.features.auto_captcha_resume, captchaResume);
      assert.equal(toForm(saved).features.nolCaptchaDetection, nolCaptchaDetection);
      assert.equal(toForm(saved).features.captchaResume, captchaResume);
    }
  }
});

test('manual entry is independently optional and preserves the initial and follow-up button settings', () => {
  const previous = mergeProfile({
    features: { auto_entry: true },
    entry_target: { type: 'point', point: { x: 20, y: 30 } },
    legacy: { date: { type: 'point', point: { x: 50, y: 60 } },
      session: { type: 'point', point: { x: 70, y: 80 } },
      next: { type: 'point', point: { x: 90, y: 100 } } },
  });
  assert.equal(previous.features.manual_entry, false);
  assert.equal(normalizeSnapshot({ phase: 'captcha', manualEntry: true }).label, '날짜·회차 직접 진행 대기');
  assert.notEqual(normalizeSnapshot({ phase: 'captcha', manualEntry: true, nolCaptcha: { enabled: true } }).label,
    '날짜·회차 직접 진행 대기');
  for (const enabled of [false, true]) {
    for (const nol of [false, true]) {
      const form = toForm(previous);
      form.features.manualEntry = enabled;
      form.features.nolCaptchaDetection = nol;
      const saved = fromForm(form, previous);
      assert.equal(saved.features.manual_entry, enabled);
      assert.equal(saved.features.nol_captcha_detection, nol);
      assert.equal(saved.features.auto_captcha_resume, false);
      assert.deepEqual(saved.entry_target, previous.entry_target);
      for (const key of ['date', 'session', 'next']) assert.deepEqual(saved.legacy[key], previous.legacy[key]);
      assert.equal(toForm(mergeProfile(saved)).features.manualEntry, enabled);
    }
  }
});

test('adjacent-seat preference is independent, defaults off for older settings, and preserves target counts', () => {
  const previous = mergeProfile({ target_seats: 3, features: { seat_priority: true } });
  assert.equal(previous.features.adjacent_seats, false);
  assert.equal(FEATURE_OPTIONS.find(([key]) => key === 'adjacentSeats')[1], '연석 우선');
  for (const targetCount of ['2', '3']) {
    for (const adjacentSeats of [false, true]) {
      const form = toForm(previous);
      form.targetCount = targetCount;
      form.features.adjacentSeats = adjacentSeats;
      const saved = fromForm(form, previous);
      assert.equal(saved.target_seats, Number(targetCount));
      assert.equal(saved.features.adjacent_seats, adjacentSeats);
      assert.equal(saved.features.seat_priority, true);
      assert.equal(toForm(mergeProfile(saved)).features.adjacentSeats, adjacentSeats);
      assert.equal(saved.features.automatic_refresh, false);
    }
  }
});

test('form serialization preserves runtime metadata and uses the core snake_case contract', () => {
  const previous = defaultProfile();
  previous.desktop.capture_reference = { width: 1800, height: 1200, scale_x: 2, scale_y: 2 };
  previous.desktop.future_adapter = { enabled: true };
  previous.sale_deadline_ms = 12345;
  const form = toForm(previous);
  form.targetCount = '4';
  form.features.conflictRecovery = true;
  form.features.regionPriority = true;
  form.seatColor = '#00a3ef';
  form.regionColor = '#c7d520';
  const result = fromForm(form, previous);
  assert.equal(result.target_seats, 4);
  assert.equal(result.features.conflict_recovery, true);
  assert.equal(result.features.region_priority, true);
  assert.equal(result.features.auto_entry, false);
  assert.deepEqual(result.color, { r: 0, g: 163, b: 239 });
  assert.deepEqual(result.region_color, { r: 199, g: 213, b: 32 });
  assert.deepEqual(result.desktop.capture_reference, previous.desktop.capture_reference);
  assert.deepEqual(result.desktop.future_adapter, { enabled: true });
  assert.equal(result.sale_deadline_ms, null, 'a persisted monotonic deadline must never be reused');
});

test('sale time round trips in the local timezone, and scheduling only requires its own fields', () => {
  const p = defaultProfile();
  const form = toForm(p);
  form.saleAt = '2026-10-22T12:00:00';
  form.features.scheduledStart = true;
  const result = fromForm(form, p);
  result.entry_target = { type: 'point', point: { x: 300, y: 400 } };
  assert.equal(toLocalDateTime(result.desktop.sale_time), form.saleAt);
  assert.doesNotThrow(() => validateStart(result));
  result.desktop.sale_time = null;
  assert.throws(() => validateStart(result), /예약 시작/);
  result.desktop.scheduled_start = false;
  result.features.server_clock = true;
  assert.doesNotThrow(() => validateStart(result), 'Immediate runs do not use a saved clock option');
  result.desktop.scheduled_start = true;
  result.desktop.sale_time = '2026-10-22T03:00:00Z';
  assert.throws(() => validateStart(result), /공연 페이지 주소/);
});

test('invalid counts, colors and active URLs receive matching user-facing field names', () => {
  const profile = defaultProfile();
  const form = toForm(profile);
  assert.throws(() => fromForm({ ...form, targetCount: '5' }, profile), /목표 좌석/);
  assert.throws(() => fromForm({ ...form, tolerance: '' }, profile), /색상 허용 오차/);
  assert.throws(() => fromForm({ ...form, seatColor: '#123' }, profile), /색상 값/);
  assert.throws(() => fromForm({ ...form, url: 'javascript:alert(1)' }, profile), /공연 페이지 주소/);
});

test('capture coordinates remain window-relative and rectangle corners may be supplied in either order', () => {
  const first = applyCapture(defaultProfile(), {
    field: 'vision.roi.start', point: { x: 900, y: 600 },
    frame: { width: 1200, height: 800, scale_x: 2, scale_y: 2 },
  });
  assert.equal(first.vision.roi, null);
  const second = applyCapture(first, { field: 'vision.roi.end', point: { x: 100, y: 150 } });
  assert.deepEqual(second.vision.roi, { x: 100, y: 150, width: 800, height: 450 });
  assert.equal(second.desktop.capture_reference.scale_x, 2);
  assert.equal(clearCapture(second, 'vision.roi.end').vision.roi, null);
  assert.throws(() => applyCapture(second, { field: 'legacy.complete', point: { x: -1, y: 3 } }), /대상 예매창/);
});

test('original two-region navigation and arrow-key navigation survive save/load', () => {
  const original = fromForm({ ...toForm(defaultProfile()), refreshMode: 'legacyRegions' }, defaultProfile());
  let profile = applyCapture(original, { field: 'legacy.region_first', point: { x: 20, y: 30 } });
  assert.equal(profile.legacy.cancellation_navigation.type, 'none');
  profile = applyCapture(profile, { field: 'legacy.region_second', point: { x: 50, y: 60 } });
  assert.deepEqual(profile.legacy.cancellation_navigation, {
    type: 'two_regions',
    first: { type: 'point', point: { x: 20, y: 30 } },
    second: { type: 'point', point: { x: 50, y: 60 } },
  });
  const form = toForm(profile);
  form.refreshMode = 'legacyKeys';
  const saved = fromForm(form, profile);
  assert.equal(saved.legacy.cancellation_navigation.type, 'arrow_keys');
  assert.equal(toForm(saved).refreshMode, 'legacyKeys');
});

test('legacy profiles with two-region targets and no desktop metadata retain their targets', () => {
  const profile = mergeProfile({
    schema_version: 1,
    legacy: {
      cancellation_navigation: {
        type: 'two_regions',
        first: { type: 'point', point: { x: 20, y: 30 } },
        second: { type: 'point', point: { x: 50, y: 60 } },
      },
    },
  });
  const saved = fromForm(toForm(profile), profile);
  assert.deepEqual(saved.legacy.cancellation_navigation, profile.legacy.cancellation_navigation);
});

test('selected count uses confirmed IDs and never counts attempted inputs as selected', () => {
  const state = normalizeSnapshot({
    phase: 'seats', selected_ids: ['retained'], attempted_ids: ['retained', 'unconfirmed'],
    current_candidate_id: 'unconfirmed', target_seats: 2,
  });
  assert.equal(state.selectedCount, 1);
  assert.equal(state.targetCount, 2);
  assert.equal(state.running, true);
  assert.equal(normalizeSnapshot({ phase: 'completed' }).running, false);
});

test('import rejects unsupported schemas and ignores prototype pollution keys', () => {
  assert.throws(() => mergeProfile({ schema_version: 300 }), /설정 버전/);
  const profile = mergeProfile(JSON.parse('{"__proto__":{"polluted":true},"desktop":{"constructor":{"bad":true}}}'));
  assert.equal({}.polluted, undefined);
  assert.equal(Object.hasOwn(profile.desktop, 'constructor'), false);
});

test('optional per-zone overrides stay empty until the current zone is configured', () => {
  const profile = defaultProfile();
  const saved = fromForm(toForm(profile), profile);
  assert.deepEqual(saved.legacy.regions, []);
  assert.equal(toForm(profile).zone0ColorEnabled, false);
  assert.equal(toForm(profile).zone1ColorEnabled, false);
  assert.doesNotThrow(() => validateStart(saved));
});

test('two cancellation zones retain independent colors, search bounds, and completion buttons', () => {
  let profile = defaultProfile();
  profile = applyCapture(profile, { field: 'legacy.regions.0.color', color: { r: 200, g: 40, b: 20 } });
  profile = applyCapture(profile, { field: 'legacy.regions.1.color', color: { r: 20, g: 160, b: 210 } });
  profile = applyCapture(profile, { field: 'legacy.regions.0.vision.roi.start', point: { x: 10, y: 20 } });
  profile = applyCapture(profile, { field: 'legacy.regions.0.vision.roi.end', point: { x: 210, y: 320 } });
  profile = applyCapture(profile, { field: 'legacy.regions.1.vision.roi.start', point: { x: 350, y: 440 } });
  profile = applyCapture(profile, { field: 'legacy.regions.1.vision.roi.end', point: { x: 150, y: 240 } });
  profile = applyCapture(profile, { field: 'legacy.regions.0.complete', point: { x: 500, y: 600 } });
  profile = applyCapture(profile, { field: 'legacy.regions.1.complete', point: { x: 700, y: 800 } });
  const saved = fromForm(toForm(profile), profile);
  const restored = mergeProfile(JSON.parse(JSON.stringify(saved)));
  assert.deepEqual(restored.legacy.regions, profile.legacy.regions);
  assert.deepEqual(restored.legacy.regions[0].vision.roi, { x: 10, y: 20, width: 200, height: 300 });
  assert.deepEqual(restored.legacy.regions[1].vision.roi, { x: 150, y: 240, width: 200, height: 200 });
  assert.deepEqual(restored.desktop.capture_points.zone1_start, { x: 350, y: 440 });
  assert.equal(restored.color.r, 130, 'per-zone colors must not replace the common color');
  assert.equal(toForm(restored).zone0ColorEnabled, true);
  assert.equal(toForm(restored).zone1Color, '#14a0d2');
});

test('clearing one zone override restores its common fallback and keeps unrelated overrides', () => {
  let profile = applyCapture(defaultProfile(), { field: 'legacy.regions.1.color', color: { r: 4, g: 5, b: 6 } });
  profile = applyCapture(profile, { field: 'legacy.regions.1.complete', point: { x: 90, y: 100 } });
  profile = applyCapture(profile, { field: 'legacy.regions.1.vision.roi.start', point: { x: 10, y: 20 } });
  profile = applyCapture(profile, { field: 'legacy.regions.1.vision.roi.end', point: { x: 50, y: 70 } });
  assert.deepEqual(profile.legacy.regions[0], { color: null, vision: null, complete: null });
  profile = clearCapture(profile, 'legacy.regions.1.vision.roi.start');
  assert.equal(profile.legacy.regions[1].vision, null);
  assert.deepEqual(profile.legacy.regions[1].color, { r: 4, g: 5, b: 6 });
  const form = toForm(profile);
  form.zone1ColorEnabled = false;
  profile = fromForm(form, profile);
  assert.equal(profile.legacy.regions[1].color, null);
  assert.deepEqual(profile.legacy.regions[1].complete.point, { x: 90, y: 100 });
  assert.equal(clearCapture(profile, 'legacy.regions.1.complete').legacy.regions[1].complete, null);
});

test('imported per-zone rectangles display correctly without capture metadata', () => {
  const profile = mergeProfile({
    legacy: { regions: [{ color: null, complete: null, vision: { roi: { x: 20, y: 30, width: 100, height: 200 }, exclusions: [], min_component_pixels: 3, max_component_pixels: 100000 } }] },
  });
  assert.deepEqual(captureValue(profile, 'legacy.regions.0.vision.roi.start'), { x: 20, y: 30 });
  assert.deepEqual(captureValue(profile, 'legacy.regions.0.vision.roi.end'), { x: 120, y: 230 });
  assert.throws(() => mergeProfile({ legacy: { regions: [{}, {}, {}] } }), /최대 2개/);
});

test('a color-only capture after a 2x to 1x screen change invalidates every old coordinate', () => {
  const profile = defaultProfile();
  const target = { type: 'point', point: { x: 400, y: 300 } };
  const vision = { roi: { x: 100, y: 120, width: 600, height: 400 }, exclusions: [{ x: 10, y: 20, width: 40, height: 50 }], min_component_pixels: 3, max_component_pixels: 100000 };
  Object.assign(profile.legacy, {
    date: target, session: target, next: target, complete: target,
    cancellation_navigation: { type: 'two_regions', first: target, second: target },
    regions: [{ color: { r: 1, g: 2, b: 3 }, vision, complete: target }],
  });
  profile.vision = vision;
  profile.refresh_target = target;
  profile.entry_target = target;
  Object.assign(profile.desktop, {
    capture_reference: { width: 2000, height: 1400, scale_x: 2, scale_y: 2 },
    capture_points: { seat_start: { x: 100, y: 120 }, zone0_start: { x: 100, y: 120 } },
    stage_center: { x: 800, y: 100 },
    region_roi: { x: 100, y: 200, width: 900, height: 600 },
  });
  const result = applyCapture(profile, {
    field: 'color', color: { r: 40, g: 50, b: 60 },
    frame: { width: 1000, height: 700, scale_x: 1, scale_y: 1 },
  });
  for (const key of ['date', 'session', 'next', 'complete']) assert.equal(result.legacy[key], null);
  assert.equal(result.legacy.cancellation_navigation.type, 'none');
  assert.equal(result.legacy.regions[0].vision, null);
  assert.equal(result.legacy.regions[0].complete, null);
  assert.deepEqual(result.legacy.regions[0].color, { r: 1, g: 2, b: 3 });
  assert.equal(result.vision.roi, null);
  assert.deepEqual(result.vision.exclusions, []);
  assert.equal(result.refresh_target, null);
  assert.equal(result.entry_target, null);
  assert.equal(result.desktop.stage_center, null);
  assert.equal(result.desktop.region_roi, null);
  assert.deepEqual(result.desktop.capture_points, {});
  assert.deepEqual(result.desktop.capture_reference, { width: 1000, height: 700, scale_x: 1, scale_y: 1 });
  assert.deepEqual(result.color, { r: 40, g: 50, b: 60 });
  assert.deepEqual(profile.legacy.date, target, 'the input profile remains untouched');
});

test('changed geometry preserves keyboard navigation and applies only the new point', () => {
  const profile = defaultProfile();
  profile.legacy.cancellation_navigation = { type: 'arrow_keys' };
  profile.desktop.capture_reference = { width: 1000, height: 700, scale_x: 2, scale_y: 2 };
  profile.desktop.capture_points = { zone1_end: { x: 800, y: 600 } };
  const result = applyCapture(profile, {
    field: 'legacy.regions.1.vision.roi.start', point: { x: 20, y: 30 },
    frame: { width: 1000, height: 700, scale_x: 1, scale_y: 1 },
  });
  assert.equal(result.legacy.cancellation_navigation.type, 'arrow_keys');
  assert.deepEqual(result.desktop.capture_points, { zone1_start: { x: 20, y: 30 } });
  assert.equal(result.legacy.regions[1].vision, null, 'an old opposite corner cannot complete the new rectangle');
});

test('unchanged capture geometry preserves coordinates during color calibration', () => {
  const profile = defaultProfile();
  const frame = { width: 1000, height: 700, scale_x: 2, scale_y: 2 };
  profile.desktop.capture_reference = frame;
  profile.legacy.complete = { type: 'point', point: { x: 200, y: 300 } };
  const result = applyCapture(profile, { field: 'color', color: { r: 1, g: 2, b: 3 }, frame: { ...frame } });
  assert.deepEqual(result.legacy.complete, profile.legacy.complete);
});

test('small scale rounding matches the native runtime geometry tolerance', () => {
  const profile = defaultProfile();
  profile.desktop.capture_reference = { width: 1000, height: 700, scale_x: 2, scale_y: 2 };
  profile.legacy.complete = { type: 'point', point: { x: 200, y: 300 } };
  const result = applyCapture(profile, {
    field: 'color', color: { r: 1, g: 2, b: 3 },
    frame: { width: 1000, height: 700, scale_x: 2.0005, scale_y: 2 },
  });
  assert.deepEqual(result.legacy.complete, profile.legacy.complete);
});

test('new zone bounds inherit common vision exclusions exactly like native capture', () => {
  const profile = defaultProfile();
  profile.vision.exclusions = [{ x: 300, y: 100, width: 100, height: 50 }];
  const first = applyCapture(profile, { field: 'legacy.regions.0.vision.roi.start', point: { x: 20, y: 30 } });
  const result = applyCapture(first, { field: 'legacy.regions.0.vision.roi.end', point: { x: 500, y: 400 } });
  assert.deepEqual(result.legacy.regions[0].vision.exclusions, profile.vision.exclusions);
  assert.equal(result.legacy.regions[0].vision.min_component_pixels, profile.vision.min_component_pixels);
});

test('scheduled entry requires its visible booking button and keeps it distinct from legacy next', () => {
  let profile = defaultProfile();
  profile.desktop.scheduled_start = true;
  profile.desktop.sale_time = '2026-10-22T03:00:00Z';
  profile.legacy.next = { type: 'point', point: { x: 900, y: 700 } };
  assert.throws(() => validateStart(profile), /판매 시작 예매 버튼.*F8/);
  profile = applyCapture(profile, { field: 'entry_target', point: { x: 300, y: 400 } });
  assert.doesNotThrow(() => validateStart(profile));
  assert.deepEqual(profile.entry_target, { type: 'point', point: { x: 300, y: 400 } });
  assert.deepEqual(profile.legacy.next.point, { x: 900, y: 700 });
  const saved = fromForm(toForm(profile), profile);
  assert.deepEqual(saved.entry_target, profile.entry_target);
  assert.equal(clearCapture(saved, 'entry_target').entry_target, null);
});

test('scheduled cancellation does not require the normal-booking entry button', () => {
  const profile = defaultProfile();
  profile.desktop.scheduled_start = true;
  profile.desktop.sale_time = '2026-10-22T03:00:00Z';
  assert.doesNotThrow(() => validateStart(profile, 'cancellation'));
  assert.throws(() => validateStart(profile, 'normal'), /판매 시작 예매 버튼/);
});

test('automatic entry prerequisites apply only to normal entry and allow the captcha Enter route', () => {
  const profile = defaultProfile();
  profile.features.auto_entry = true;
  assert.throws(() => validateStart(profile, 'normal'), /예매 단계 자동 진행.*판매 시작 예매 버튼/);
  profile.entry_target = { type: 'point', point: { x: 100, y: 200 } };
  assert.doesNotThrow(() => validateStart(profile, 'normal'));
  assert.doesNotThrow(() => validateStart(profile, 'cancellation'));
  profile.entry_target = null;
  profile.features.auto_captcha_resume = true;
  assert.doesNotThrow(() => validateStart(profile, 'normal'));
  profile.features.auto_captcha_resume = false;
  profile.features.nol_captcha_detection = true;
  assert.doesNotThrow(() => validateStart(profile, 'normal'), 'NOL-only detection preserves the registered entry path');
  profile.desktop.scheduled_start = true;
  assert.throws(() => validateStart(profile, 'normal'), /예약 시작/, 'NOL detection does not skip the sale gate');
});

test('ordinary immediate booking remains valid without an entry target when automatic entry is off', () => {
  const profile = defaultProfile();
  assert.equal(profile.entry_target, null);
  assert.doesNotThrow(() => validateStart(profile, 'normal'));
  assert.doesNotThrow(() => validateStart(profile, 'cancellation'));
});

test('stable seat IDs expose positions and distinguish equal coordinates in different zones', () => {
  assert.equal(candidatePositionLabel('pixel:120:240'), '위치 120, 240');
  assert.equal(candidatePositionLabel('pixel:0:0'), '위치 0, 0');
  assert.equal(candidatePositionLabel('zone0:pixel:120:240'), '1구역 · 위치 120, 240');
  assert.equal(candidatePositionLabel('zone1:pixel:120:240'), '2구역 · 위치 120, 240');
});

test('unknown or malformed candidate IDs never invent a spatial position', () => {
  for (const id of [null, 'dom:seat-42', '3:120:240', 'zone2:pixel:10:20', 'pixel:-1:20', 'pixel:10.5:20', 'pixel:9007199254740992:20']) {
    assert.equal(candidatePositionLabel(id), '', String(id));
  }
});

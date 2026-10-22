export const FEATURE_OPTIONS = [
  ['autoEntry', '예매 단계 자동 진행', '판매 시작 버튼, 날짜·회차, 다음 예매 버튼을 현재 화면에서 찾습니다. 화면 인식 연결이 필요합니다.'],
  ['manualEntry', '날짜·회차·예매 버튼 직접 진행', '최초 예약 예매 버튼은 설정대로 누르고, 등록된 날짜·회차·다음 버튼은 건너뜁니다. 직접 진입한 뒤 캡챠 판단·Enter 또는 F9로 구역·좌석 탐색을 시작하세요.'],
  ['captchaResume', '캡챠 Enter 후 구역부터 자동 진행', '일반 예매에서 지정한 날짜·회차·다음 버튼을 먼저 누른 뒤 캡챠 Enter를 기다립니다. 미지정 단계는 직접 진행하세요. Enter를 다시 누르면 구역·좌석만 다시 찾습니다. NOL 캡챠 판단도 켜면 NOL 판정이 우선합니다.'],
  ['nolCaptchaDetection', 'NOL 캡챠 판단', '일반 예매의 진입 버튼 처리 뒤 첫 영문자·숫자 입력에서 왼쪽 아래 배경 색을 한 번 저장합니다. 첫 입력만으로 클릭하지 않고 이후 배경 색 변화로 구역·좌석을 진행합니다. Enter 옵션보다 우선하며 문자 자동 풀이는 하지 않습니다. 자동 감지가 안 되면 캡챠를 직접 완료한 뒤 F9로 구역·좌석부터 진행하세요.'],
  ['regionPriority', '무대 중앙 · 가까운 구역 우선', '지정한 색상에서 중앙 구역을 먼저 고르고, 그 안에서 무대와 가까운 순서로 선택합니다.'],
  ['seatTopPriority', '위쪽 좌석부터 선택', '같은 색상의 좌석을 화면 위에서 아래 순서로 선택합니다.'],
  ['adjacentSeats', '연석 우선', '같은 줄에 붙어 있는 것으로 보이는 좌석을 먼저 선택합니다. 원하는 매수의 연석이 없으면 떨어진 좌석으로 채웁니다. 색상 영역의 크기와 간격으로 추정하며 좌석 번호를 읽지는 않습니다.'],
  ['seatObjects', '좌석 외곽으로 구분', '주변 좌석에서 반복되는 크기와 외곽을 확인해 원형·비원형의 색상 조각을 좌석 단위로 구분합니다. 좌석 내부 색을 지정하세요. 외곽 근거가 부족한 후보는 클릭하지 않고 계속 탐색합니다.'],
  ['nolSelectionCount', 'NOL 선택 좌석 수 확인', '화면 전체에서 선택 좌석 문구와 수량을 찾아 클릭 결과를 확인합니다. 위치를 고정하지 않으며, 지원하지 않는 글꼴·표시는 읽지 못할 수 있습니다. 수량을 못 읽거나 클릭 결과가 불명확하면 추가 클릭과 완료를 보류합니다.'],
  ['conflictRecovery', '선점 실패 시 다음 좌석 시도', '선점 실패 안내가 확인되면 Esc로 닫고 다음 후보를 시도합니다. 유지된 좌석은 남겨둡니다.'],
  ['automaticRefresh', '취소표에서 버튼 사용', '취소표의 내부 버튼 방식을 선택하고 새로고침 버튼 좌표를 지정했을 때만 클릭합니다. 일반 예매와 좌석 바로 탐색에는 적용하지 않습니다.'],
  ['statusOverlay', '작은 상단 상태 표시창', '예매창 위에 현재 단계와 선택한 매수를 표시합니다. 클릭을 가리지 않습니다.'],
  ['serverClock', '서버 시간 보정', '공연 페이지 주소로 시간 차이를 추정합니다. 예약 시작 시각에 보정값을 적용합니다.'],
];

const FEATURE_MAP = {
  serverClock: 'server_clock',
  autoEntry: 'auto_entry',
  manualEntry: 'manual_entry',
  captchaResume: 'auto_captcha_resume',
  nolCaptchaDetection: 'nol_captcha_detection',
  regionPriority: 'region_priority',
  seatTopPriority: 'seat_priority',
  adjacentSeats: 'adjacent_seats',
  seatObjects: 'seat_objects',
  nolSelectionCount: 'nol_selection_count',
  conflictRecovery: 'conflict_recovery',
  automaticRefresh: 'automatic_refresh',
  statusOverlay: 'status_overlay',
};

export const PHASE_LABELS = {
  idle: '실행 대기',
  waiting_sale: '판매 시작 대기',
  waitingSale: '판매 시작 대기',
  entry: '예매 진입',
  captcha: '캡챠 입력 대기',
  region: '구역 선택',
  waiting_region: '구역 화면 표시 대기',
  seats: '좌석 탐색',
  conflict: '선점 실패 처리',
  waiting_refresh: '새로고침 대기',
  waitingRefresh: '새로고침 대기',
  completing: '완료 버튼 처리',
  paused: '일시정지',
  completed: '실행 종료',
  stopped: '중지됨',
  faulted: '확인이 필요합니다',
  unsupported: '데스크톱 앱에서 실행하세요',
};

export const RUN_MODE_LABELS = {
  seats: '좌석 바로 탐색',
  normal: '일반 예매',
  cancellation: '취소표 탐색',
};

export function selectionPolicyForProfile(profile, mode) {
  return mode === 'seats'
    || (mode === 'normal' && (profile.features?.auto_captcha_resume === true
      || profile.features?.nol_captcha_detection === true || profile.features?.manual_entry === true))
    || (mode === 'cancellation' && profile.legacy?.cancellation_navigation?.type === 'none')
    ? 'optimistic' : 'confirmed';
}

export function cancellationNavigationForMode(profile, mode) {
  if (mode === 'legacyKeys') return { type: 'arrow_keys' };
  if (mode !== 'legacyRegions') return { type: 'none' };
  const points = profile.desktop?.capture_points || {};
  if (points.legacy_region_first && points.legacy_region_second) {
    return {
      type: 'two_regions',
      first: { type: 'point', point: points.legacy_region_first },
      second: { type: 'point', point: points.legacy_region_second },
    };
  }
  return profile.legacy.cancellation_navigation?.type === 'two_regions'
    ? profile.legacy.cancellation_navigation : { type: 'none' };
}

export function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

export function defaultProfile() {
  return {
    schema_version: 1,
    name: '기본 설정',
    target_seats: 2,
    color: { r: 130, g: 70, b: 210 },
    color_tolerance: 12,
    region_color: null,
    features: Object.fromEntries(Object.values(FEATURE_MAP).map((name) => [name, false])),
    legacy: {
      date: null, session: null, next: null, complete: null,
      cancellation_navigation: { type: 'none' },
      regions: [],
      blind_click_sequence: true,
      normal_click_delay_ms: 60,
      cancellation_click_delay_ms: 60,
      region_delay_ms: 200,
    },
    desired_date: null,
    desired_session: null,
    server_url: null,
    entry_target: null,
    sale_deadline_ms: null,
    refresh_target: null,
    refresh_interval_ms: 1000,
    action_timeout_ms: 2000,
    observation_timeout_ms: 5000,
    vision: { roi: null, exclusions: [], min_component_pixels: 3, max_component_pixels: 100000 },
    desktop: {
      scheduled_start: false,
      sale_time: null,
      refresh_mode: 'none',
      cancellation_mode: 'automatic',
      region_color: { r: 130, g: 70, b: 210 },
      stage_center: null,
      region_roi: null,
      capture_points: {},
    },
  };
}

export function mergeProfile(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('설정 파일은 JSON 객체여야 합니다.');
  if (value.schema_version !== undefined && value.schema_version !== 1) throw new Error('이 앱에서 지원하지 않는 설정 버전입니다.');
  const result = defaultProfile();
  mergeObjects(result, value);
  if (!Array.isArray(result.legacy.regions) || result.legacy.regions.length > 2
    || result.legacy.regions.some((region) => !region || typeof region !== 'object' || Array.isArray(region))) {
    throw new Error('취소표 구역 설정은 최대 2개여야 합니다.');
  }
  return result;
}

function mergeObjects(target, source) {
  for (const [key, value] of Object.entries(source)) {
    if (['__proto__', 'constructor', 'prototype'].includes(key)) continue;
    if (value && typeof value === 'object' && !Array.isArray(value)
      && target[key] && typeof target[key] === 'object' && !Array.isArray(target[key])) {
      mergeObjects(target[key], value);
    } else {
      target[key] = clone(value);
    }
  }
}

export function rgbToHex(color) {
  if (!color) return '#8246d2';
  return `#${['r', 'g', 'b'].map((channel) => Math.min(255, Math.max(0, Number(color[channel]) || 0)).toString(16).padStart(2, '0')).join('')}`;
}

export function hexToRgb(hex) {
  if (!/^#[0-9a-f]{6}$/i.test(hex)) throw new Error('색상 값은 #8246D2처럼 6자리로 입력하세요.');
  return { r: parseInt(hex.slice(1, 3), 16), g: parseInt(hex.slice(3, 5), 16), b: parseInt(hex.slice(5, 7), 16) };
}

export function toLocalDateTime(value) {
  if (!value) return '';
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return '';
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 19);
}

export function toForm(profile) {
  const p = mergeProfile(profile);
  const mode = p.legacy.cancellation_navigation.type;
  return {
    name: p.name,
    targetCount: p.target_seats,
    seatColor: rgbToHex(p.color),
    regionColor: rgbToHex(p.region_color || p.desktop.region_color),
    tolerance: p.color_tolerance,
    url: p.server_url || '',
    performanceDate: p.desired_date || '',
    performanceSession: p.desired_session || '',
    saleAt: toLocalDateTime(p.desktop.sale_time),
    refreshMode: ['button', 'internalButton'].includes(p.desktop.refresh_mode) ? 'internalButton'
      : p.desktop.refresh_mode === 'automatic' ? 'automatic'
        : mode === 'arrow_keys' ? 'legacyKeys' : mode === 'two_regions' ? 'legacyRegions'
          : p.desktop.cancellation_mode || 'legacyRegions',
    clickDelayMs: p.legacy.normal_click_delay_ms,
    cancellationDelayMs: p.legacy.cancellation_click_delay_ms,
    regionDelayMs: p.legacy.region_delay_ms,
    refreshIntervalMs: p.refresh_interval_ms,
    zone0ColorEnabled: Boolean(p.legacy.regions[0]?.color),
    zone0Color: rgbToHex(p.legacy.regions[0]?.color || p.color),
    zone1ColorEnabled: Boolean(p.legacy.regions[1]?.color),
    zone1Color: rgbToHex(p.legacy.regions[1]?.color || p.color),
    features: {
      ...Object.fromEntries(Object.entries(FEATURE_MAP).map(([ui, engine]) => [ui, Boolean(p.features[engine])])),
      scheduledStart: Boolean(p.desktop.scheduled_start),
    },
  };
}

function integer(value, label, min, max) {
  if (value === '' || value === null || value === undefined) throw new Error(`${label}을(를) 입력하세요.`);
  const result = Number(value);
  if (!Number.isInteger(result) || result < min || result > max) throw new Error(`${label}은(는) ${min}–${max} 사이의 정수여야 합니다.`);
  return result;
}

export function fromForm(form, previous) {
  const p = mergeProfile(previous);
  // Older profiles store the original two targets only in navigation. Keep
  // them when switching to a single-region or keyboard mode and back later.
  const navigation = p.legacy.cancellation_navigation;
  if (navigation.type === 'two_regions') {
    for (const [key, target] of [['legacy_region_first', navigation.first], ['legacy_region_second', navigation.second]]) {
      if (!p.desktop.capture_points[key] && target?.type === 'point') p.desktop.capture_points[key] = clone(target.point);
    }
  }
  p.name = form.name.trim() || '기본 설정';
  p.target_seats = integer(form.targetCount, '목표 좌석', 1, 4);
  p.color = hexToRgb(form.seatColor);
  p.color_tolerance = integer(form.tolerance, '색상 허용 오차', 0, 255);
  p.server_url = form.url.trim() || null;
  if (p.server_url) {
    let url;
    try { url = new URL(p.server_url); } catch { throw new Error('공연 페이지 주소를 https://로 시작하는 전체 주소로 입력하세요.'); }
    if (!['https:', 'http:'].includes(url.protocol)) throw new Error('공연 페이지 주소는 http:// 또는 https://로 시작해야 합니다.');
    if (url.username || url.password) throw new Error('공연 페이지 주소에 계정 정보가 포함되어 있습니다.');
  }
  p.desired_date = form.performanceDate || null;
  p.desired_session = form.performanceSession.trim() || null;
  p.legacy.normal_click_delay_ms = integer(form.clickDelayMs, '일반 예매 클릭 간격', 0, 10000);
  p.legacy.cancellation_click_delay_ms = integer(form.cancellationDelayMs, '취소표 탐색 클릭 간격', 0, 10000);
  p.legacy.region_delay_ms = integer(form.regionDelayMs, '구역 이동 후 간격', 0, 10000);
  p.refresh_interval_ms = integer(form.refreshIntervalMs, '빈 좌석 재탐색·새로고침 간격', 100, 60000);
  for (const [ui, engine] of Object.entries(FEATURE_MAP)) p.features[engine] = Boolean(form.features[ui]);
  p.desktop.scheduled_start = Boolean(form.features.scheduledStart);
  if (form.saleAt) {
    const date = new Date(form.saleAt);
    if (!Number.isFinite(date.getTime())) throw new Error('판매 시작 시각을 확인하세요.');
    p.desktop.sale_time = date.toISOString();
  } else p.desktop.sale_time = null;
  p.sale_deadline_ms = null;
  p.desktop.region_color = hexToRgb(form.regionColor);
  p.region_color = clone(p.desktop.region_color);
  p.desktop.refresh_mode = form.refreshMode === 'automatic' ? 'automatic' : form.refreshMode === 'internalButton' ? 'button' : 'none';
  p.desktop.cancellation_mode = form.refreshMode;
  for (let index = 0; index < 2; index += 1) {
    const enabled = Boolean(form[`zone${index}ColorEnabled`]);
    if (enabled || p.legacy.regions[index]) {
      const region = ensureLegacyRegion(p, index);
      region.color = enabled ? hexToRgb(form[`zone${index}Color`]) : null;
    }
  }
  p.legacy.cancellation_navigation = cancellationNavigationForMode(p, form.refreshMode);
  return p;
}

export function validateStart(profile, mode = 'normal') {
  if (mode === 'cancellation' && profile.desktop.cancellation_mode === 'legacyRegions') {
    const navigation = profile.legacy.cancellation_navigation;
    const pointTarget = target => target?.type === 'point'
      && Number.isFinite(target.point?.x) && Number.isFinite(target.point?.y)
      && target.point.x >= 0 && target.point.y >= 0;
    if (navigation.type !== 'two_regions' || !pointTarget(navigation.first) || !pointTarget(navigation.second)) {
      throw new Error('취소표 원본 두 구역: ‘원본 · 첫 번째 구역’과 ‘원본 · 두 번째 구역’을 F8로 지정하세요. 현재 구역만 탐색하려면 ‘한 구역 · 현재 좌석 계속 탐색’을 선택하세요.');
    }
  }
  if (mode === 'seats') {
    const roi = profile.vision?.roi;
    if (!roi || !['x', 'y', 'width', 'height'].every((key) => Number.isFinite(roi[key]))
      || roi.x < 0 || roi.y < 0 || roi.width <= 0 || roi.height <= 0) {
      throw new Error('좌석 바로 탐색: ‘좌석 영역 · 첫 모서리’와 ‘좌석 영역 · 반대 모서리’를 F8로 지정하세요.');
    }
    return;
  }
  if (mode !== 'normal') return;
  if (profile.desktop.scheduled_start && !profile.desktop.sale_time) throw new Error('예약 시작: 판매 시작 시각을 입력하세요.');
  if (profile.desktop.scheduled_start && !profile.entry_target) throw new Error('예약 시작: ‘판매 시작 예매 버튼’을 F8로 지정하세요. 시간 측정만으로 클릭할 버튼이 정해지지는 않습니다.');
  if (profile.features.auto_entry && !profile.entry_target
    && !profile.features.auto_captcha_resume && !profile.features.nol_captcha_detection && !profile.features.manual_entry) {
    throw new Error('예매 단계 자동 진행: 브라우저 자동 인식 연결이 아직 없습니다. 기능을 끄거나 일반 예매의 ‘판매 시작 예매 버튼’을 지정하세요.');
  }
  if (profile.desktop.scheduled_start && profile.features.server_clock && !profile.server_url) throw new Error('서버 시간 보정: 공연 페이지 주소를 입력하세요.');
}

export function setPath(object, path, value) {
  const keys = path.split('.');
  if (keys.some((key) => ['__proto__', 'constructor', 'prototype'].includes(key))) throw new Error('지원하지 않는 설정 항목입니다.');
  let target = object;
  for (const key of keys.slice(0, -1)) target = target[key] ??= {};
  target[keys.at(-1)] = value;
}

export function getPath(object, path) {
  return path.split('.').reduce((value, key) => value?.[key], object);
}

export function pointLabel(point) {
  if (!point) return '미지정';
  const p = point.type === 'point' ? point.point : point;
  if (point.type === 'element') return '화면 요소 지정됨';
  return Number.isFinite(p.x) && Number.isFinite(p.y) ? `${Math.round(p.x)}, ${Math.round(p.y)}` : '미지정';
}

export function candidatePositionLabel(id) {
  if (typeof id !== 'string') return '';
  const match = /^(?:zone([01]):)?pixel:(\d+):(\d+)$/.exec(id);
  if (!match) return '';
  const x = Number(match[2]);
  const y = Number(match[3]);
  if (!Number.isSafeInteger(x) || !Number.isSafeInteger(y)) return '';
  const zone = match[1] === undefined ? '' : `${Number(match[1]) + 1}구역 · `;
  return `${zone}위치 ${x}, ${y}`;
}

export function visionDiagnosticsView(diagnostic) {
  if (!diagnostic || !['seats', 'region'].includes(diagnostic.phase)) return null;
  const stats = diagnostic.stats;
  const counts = ['matching_pixels', 'components_found', 'accepted_count', 'rejected_too_small', 'rejected_too_large'];
  if (!stats || counts.some((key) => !Number.isSafeInteger(stats[key]) || stats[key] < 0)) return null;
  if (!diagnostic.color || ['r', 'g', 'b'].some((key) => !Number.isInteger(diagnostic.color[key])
    || diagnostic.color[key] < 0 || diagnostic.color[key] > 255)) return null;
  const zone = /^legacy_region_([01])$/.exec(diagnostic.source || '');
  const source = zone ? `취소표 ${Number(zone[1]) + 1}구역 탐색`
    : diagnostic.source === 'global' ? '공통 설정'
      : diagnostic.source === 'region_map' ? '구역 선택 설정' : '현재 탐색 설정';
  const color = rgbToHex(diagnostic.color).toUpperCase();
  const tolerance = Number.isInteger(diagnostic.tolerance) && diagnostic.tolerance >= 0
    && diagnostic.tolerance <= 255 ? ` · 허용 오차 ${diagnostic.tolerance}` : '';
  const awaitingRefreshImage = diagnostic.awaitingRefreshImage === true;
  const objects = diagnostic.objectStats;
  const objectMode = objects && Number.isSafeInteger(objects.resolved_objects) && objects.resolved_objects >= 0
    && Number.isSafeInteger(objects.ambiguous_objects) && objects.ambiguous_objects >= 0;
  let reason;
  if (objectMode) {
    reason = `${awaitingRefreshImage ? '새로고침 전 화면의 참고 수치입니다. ' : ''}일치 픽셀 ${stats.matching_pixels}개 · 색상 성분 ${stats.components_found}개에서 좌석 객체 후보 ${stats.accepted_count}개를 찾았습니다.`;
    if (awaitingRefreshImage) reason += ' 새 화면이 들어오면 클릭 후보를 다시 판단합니다.';
    else if (!stats.accepted_count) reason += ' 외곽·색상·관측 범위의 근거가 부족한 후보는 클릭하지 않고 계속 탐색합니다.';
  } else if (awaitingRefreshImage) {
    reason = `새로고침 전 화면의 참고 수치입니다. 일치 픽셀 ${stats.matching_pixels}개 · 색상 영역 ${stats.components_found}개 · 크기 조건 통과 ${stats.accepted_count}개. 새 화면이 들어오면 클릭 후보를 다시 판단합니다.`;
  } else if (stats.matching_pixels === 0) {
    reason = '일치 픽셀 0개 · 지정한 색상과 허용 오차에 맞는 픽셀이 없습니다. 색상과 탐색 범위를 확인하세요.';
  } else if (stats.accepted_count === 0) {
    reason = `일치 픽셀 ${stats.matching_pixels}개 · 색상 영역 ${stats.components_found}개를 찾았지만 후보 크기 조건을 통과하지 못했습니다.`;
  } else {
    reason = `일치 픽셀 ${stats.matching_pixels}개 · 색상 영역 ${stats.components_found}개 중 ${stats.accepted_count}개가 후보 크기 조건을 통과했습니다.`;
  }
  if (stats.rejected_too_small || stats.rejected_too_large) {
    reason += ` 크기 제외: 너무 작음 ${stats.rejected_too_small}개 · 너무 큼 ${stats.rejected_too_large}개.`;
  }
  if (Number.isSafeInteger(stats.rejected_unsupported) && stats.rejected_unsupported > 0) {
    reason += ` 클릭할 면적이 부족한 색상 조각 ${stats.rejected_unsupported}개는 제외했습니다. 좌석 내부의 색을 지정하세요.`;
  }
  if (objectMode) {
    reason += ` 좌석 외곽 확인 ${objects.resolved_objects}개 · 모호한 객체 ${objects.ambiguous_objects}개.`;
    if (!objects.reference_objects) reason += ' 반복되는 좌석의 크기와 외곽을 확인하지 못해 클릭을 보류하고 계속 탐색합니다.';
    if (Number.isSafeInteger(objects.search_budget_exhausted) && objects.search_budget_exhausted > 0) {
      reason += ` 큰 연결 영역 ${objects.search_budget_exhausted}개에서 일부 외곽 검사를 한도 초과로 보류했습니다. 좌석 탐색 범위를 좁혀 다시 확인할 수 있습니다.`;
    }
    const names = { outline_only_color: '외곽선 색만 일치', color_used_for_fill_and_outline: '등록 색이 외곽과 내부에 함께 쓰임',
      internal_background_mark: '내부 배경색 표시로 상태 불명확', incomplete_object: '객체 일부가 가려지거나 탐색 범위에서 제외됨',
      overlapping_hypotheses: '좌석 외곽 해석 충돌', unsupported_shape: '지원하지 않는 외곽', background_uncertain: '배경 구분 불가' };
    const details = Object.entries(diagnostic.objectReasons || {}).filter(([key, count]) => names[key] && Number.isSafeInteger(count) && count > 0)
      .map(([key, count]) => `${names[key]} ${count}개`);
    if (details.length) reason += ` 제외 근거: ${details.join(' · ')}.`;
  }
  const roi = diagnostic.roi;
  const frame = diagnostic.frame;
  const displayNumber = (value) => Math.round(value * 10) / 10;
  const geometry = [];
  if (roi && ['x', 'y', 'width', 'height'].every((key) => Number.isFinite(roi[key]))) {
    geometry.push(`탐색 범위 ${displayNumber(roi.x)}, ${displayNumber(roi.y)} / ${displayNumber(roi.width)} × ${displayNumber(roi.height)}`);
  }
  if (frame && Number.isFinite(frame.width) && Number.isFinite(frame.height)) {
    geometry.push(`캡처 ${frame.width} × ${frame.height}`);
    if (Number.isFinite(frame.scaleX) && Number.isFinite(frame.scaleY)) {
      geometry.push(`배율 ${displayNumber(frame.scaleX)} × ${displayNumber(frame.scaleY)}`);
    }
  }
  return {
    phase: diagnostic.phase,
    heading: awaitingRefreshImage ? '새로고침 전 화면 · 새 화면 대기' : `최근 ${diagnostic.phase === 'seats' ? '좌석' : '구역'} 탐색`,
    candidates: awaitingRefreshImage ? '이전 화면 참고값' : `${objectMode ? '좌석 객체' : '색상'} 후보 ${stats.accepted_count}개`,
    settings: `${source} · ${color}${tolerance}`,
    reason,
    geometry: geometry.join(' · '),
  };
}

export function visionOverlayLabel(snapshot) {
  if (snapshot.keyboardCheck?.active === true || snapshot.phase !== 'seats' || snapshot.waitingForSeatArea === true || snapshot.captureError || snapshot.targetRecovery?.active
    || snapshot.visionDiagnostics?.awaitingRefreshImage === true
    || visualDialogView(snapshot)?.active
    || ['arming', 'armed', 'reading', 'error', 'captured'].includes(snapshot.captureState)) return '';
  const view = visionDiagnosticsView(snapshot.visionDiagnostics);
  return view?.phase === 'seats' ? view.candidates : '';
}

export function visualDialogView(snapshot) {
  const dialog = snapshot.visualDialog;
  if ((snapshot.selectionPolicy !== 'optimistic' && snapshot.selectionCountVerification !== true) || dialog?.enabled !== true
    || !['watching', 'closing', 'waiting_restore'].includes(dialog.status)
    || !['seats', 'conflict', 'completing'].includes(snapshot.phase)
    || snapshot.paused || snapshot.targetRecovery?.active || snapshot.captureError
    || ['arming', 'armed', 'reading', 'error', 'captured'].includes(snapshot.captureState)) return null;
  const labels = {
    watching: '안내창 자동 대응 · 감시 중',
    closing: '안내창 후보 감지 · Esc 처리 중',
    waiting_restore: '화면 복귀 확인 중',
  };
  const message = typeof dialog.message === 'string' && dialog.message.trim()
    ? dialog.message : labels[dialog.status];
  const changedPercent = Number.isFinite(dialog.changedPercent)
    && dialog.changedPercent >= 0 && dialog.changedPercent <= 100
    ? dialog.changedPercent : null;
  return {
    status: dialog.status,
    active: dialog.status !== 'watching',
    label: labels[dialog.status],
    message,
    detail: `${message}${changedPercent === null ? '' : ` · 전체 화면 변화 ${Math.round(changedPercent * 10) / 10}%`}`,
  };
}

export function enterWatchView(snapshot) {
  const watch = snapshot.enterWatch;
  if (snapshot.nolCaptcha?.enabled === true || watch?.enabled !== true || watch.armed !== true
    || !['entry', 'captcha', 'region', 'waiting_region', 'seats', 'conflict', 'completing'].includes(snapshot.phase)
    || snapshot.paused || snapshot.targetRecovery?.active || snapshot.captureError
    || ['arming', 'armed', 'reading', 'error', 'captured'].includes(snapshot.captureState)) return null;
  return typeof watch.message === 'string' && watch.message.trim()
    ? watch.message : '캡챠 Enter 감시 중 · Enter를 누를 때마다 구역부터 다시 찾습니다.';
}

export function nolCaptchaView(snapshot) {
  const detection = snapshot.nolCaptcha;
  if (snapshot.mode !== 'normal' || detection?.enabled !== true
    || !['searching', 'visible', 'closing', 'dismissed'].includes(detection.status)
    || !['entry', 'captcha'].includes(snapshot.phase)
    || snapshot.paused || snapshot.targetRecovery?.active || snapshot.captureError
    || ['arming', 'armed', 'reading', 'error', 'captured'].includes(snapshot.captureState)) return null;
  return typeof detection.message === 'string' && detection.message.trim()
    ? detection.message : 'NOL 캡챠 판단 중 · 첫 영문자·숫자 입력에서 기준 색을 저장한 뒤 배경 색 변화를 확인합니다.';
}

export function targetRecoveryView(snapshot) {
  if (snapshot.targetRecovery?.active !== true) return null;
  const reason = typeof snapshot.targetRecovery.reason === 'string' ? snapshot.targetRecovery.reason : '';
  return {
    label: '예매창 복귀 대기 · 자동 재개',
    message: `${reason ? `${reason} ` : ''}연결한 예매창을 다시 전면에 두면 중단된 단계를 이어갑니다. 일시정지를 누르면 자동 재개를 취소합니다.`,
  };
}

export function keyboardCheckView(snapshot) {
  const check = snapshot.keyboardCheck || {};
  const count = (value) => Number.isSafeInteger(value) && value >= 0 ? value : 0;
  const accepted = count(check.accepted);
  const labels = {
    idle: '키보드 검사 대기', listening: '키보드 수신 확인 중',
    received: accepted > 0 ? '키보드 수신 확인됨' : '수신 결과 확인 필요',
    no_input: '입력이 수신되지 않았습니다', filtered: '입력이 감지됐지만 대상 확인 실패',
    error: '키보드 검사 오류', cancelled: '키보드 검사 중지됨',
  };
  const status = Object.hasOwn(labels, check.status) ? check.status : 'idle';
  const reasons = {
    own_source: '입력 출처 제외', recipient_mismatch: '수신 앱 불일치',
    app_mismatch: '연결 앱 불일치', window_mismatch: '연결 창 불일치',
    target_unavailable: '연결 창 없음', target_not_foreground: '연결 창이 앞에 없음',
    own_front_app: '매크로 앱에 입력됨', front_app_unavailable: '입력 앱 확인 불가',
    stale_event: '입력 유효시간 초과', stale_token: '이전 검사 입력',
    modifier_pressed: '조합 키 제외', repeated: '키 반복 제외',
    inactive_monitor: '감시 종료 후 입력', invalid_event: '입력 정보 오류',
    future_event: '입력 시각 오류',
  };
  const rejectionEntries = Object.entries(check.rejections || {}).filter(([, value]) => count(value) > 0);
  const rejections = rejectionEntries.map(([reason, value]) => {
    const code = /^(callback|queue|runtime):(.+)$/.exec(reason)?.[2] || reason;
    return `${reasons[code] || reason} ${value}회`;
  }).join(' · ');
  const environment = check.environment || {};
  const permission = (value) => value === true ? '허용' : value === false ? '미허용' : '확인 불가';
  const secure = environment.secureEventInput === true ? '켜짐'
    : environment.secureEventInput === false ? '꺼짐' : '확인 불가';
  const environmentLabel = environment.platform && environment.platform !== 'macos'
    ? `${environment.platform === 'windows' ? 'Windows' : environment.platform} · macOS 권한 항목은 적용되지 않습니다.`
    : `손쉬운 사용: ${permission(environment.accessibilityTrusted)} · 입력 모니터링: ${permission(environment.listenEventAccess)} · 보안 키 입력: ${secure}`;
  const message = typeof check.message === 'string' && check.message.trim() ? check.message
    : '브라우저를 연결한 뒤 검사를 시작하세요.';
  // Keep the final result above an older F8 message. A subsequent capture or
  // booking operation supplies its own message/phase and takes precedence.
  const present = check.active === true || (status !== 'idle' && snapshot.message === message
    && snapshot.running !== true && snapshot.paused !== true
    && ['idle', 'stopped', 'completed', 'faulted', 'unsupported'].includes(snapshot.phase || 'idle'));
  const callbacks = environment.platform === 'windows' ? 'OS 감지: 제공 안 됨' : `OS 감지 ${count(check.callbacks)}회`;
  return {
    active: check.active === true, present, status, accepted,
    label: labels[status],
    message,
    seconds: Math.ceil(Math.min(20_000, count(check.remainingMs)) / 1000),
    counts: `${callbacks} · 수신 ${count(check.received)}회 · 대상 확인 ${accepted}회`,
    rejections,
    environment: environmentLabel,
    warning: ['no_input', 'filtered', 'error'].includes(status),
  };
}

function rectangle(first, second) {
  const x = Math.min(first.x, second.x);
  const y = Math.min(first.y, second.y);
  const width = Math.abs(first.x - second.x);
  const height = Math.abs(first.y - second.y);
  if (!width || !height) throw new Error('탐색 영역의 두 모서리는 서로 다른 위치여야 합니다.');
  return { x, y, width, height };
}

function ensureLegacyRegion(profile, index) {
  while (profile.legacy.regions.length <= index) {
    profile.legacy.regions.push({ color: null, vision: null, complete: null });
  }
  return profile.legacy.regions[index];
}

function regionCapture(field) {
  const match = /^legacy\.regions\.([01])\.(color|complete|vision\.roi\.(start|end))$/.exec(field);
  return match ? { index: Number(match[1]), kind: match[2], edge: match[3] } : null;
}

function geometryChanged(previous, next) {
  if (!previous || !next) return false;
  const keys = ['width', 'height', 'scale_x', 'scale_y'];
  if (keys.some((key) => !Number.isFinite(Number(previous[key])) || !Number.isFinite(Number(next[key])))) return true;
  return Number(previous.width) !== Number(next.width) || Number(previous.height) !== Number(next.height)
    || Math.abs(Number(previous.scale_x) - Number(next.scale_x)) > 0.001
    || Math.abs(Number(previous.scale_y) - Number(next.scale_y)) > 0.001;
}

function invalidateCoordinates(profile) {
  for (const key of ['date', 'session', 'next', 'complete']) profile.legacy[key] = null;
  if (profile.legacy.cancellation_navigation.type === 'two_regions') {
    profile.legacy.cancellation_navigation = { type: 'none' };
  }
  for (const region of profile.legacy.regions) {
    region.vision = null;
    region.complete = null;
  }
  profile.vision.roi = null;
  profile.vision.exclusions = [];
  profile.desktop.stage_center = null;
  profile.desktop.region_roi = null;
  profile.desktop.capture_points = {};
  profile.refresh_target = null;
  profile.entry_target = null;
}

export function applyCapture(profile, capture) {
  const p = mergeProfile(profile);
  const { field, point, color, frame } = capture;
  if (geometryChanged(p.desktop.capture_reference, frame)) invalidateCoordinates(p);
  const zone = regionCapture(field);
  if (field === 'color' || field === 'desktop.region_color' || field === 'desktop.selected_color' || zone?.kind === 'color') {
    if (!color || ['r', 'g', 'b'].some((key) => !Number.isInteger(color[key]) || color[key] < 0 || color[key] > 255)) throw new Error('화면 색상을 가져오지 못했습니다.');
    if (zone) ensureLegacyRegion(p, zone.index).color = clone(color);
    else setPath(p, field, clone(color));
    if (field === 'desktop.region_color') p.region_color = clone(color);
  } else {
    if (!point || !Number.isFinite(point.x) || !Number.isFinite(point.y) || point.x < 0 || point.y < 0) throw new Error('대상 예매창 안의 위치를 지정하세요.');
    if (zone) {
      const region = ensureLegacyRegion(p, zone.index);
      if (zone.kind === 'complete') region.complete = { type: 'point', point: clone(point) };
      else {
        const points = p.desktop.capture_points;
        points[`zone${zone.index}_${zone.edge}`] = clone(point);
        const first = points[`zone${zone.index}_start`];
        const second = points[`zone${zone.index}_end`];
        if (first && second) {
          region.vision = {
            ...(region.vision || clone(p.vision)),
            roi: rectangle(first, second),
          };
        }
      }
      if (frame) p.desktop.capture_reference = clone(frame);
      return p;
    }
    const pointFields = {
      'vision.roi.start': 'seat_start', 'vision.roi.end': 'seat_end',
      'desktop.region_roi.start': 'region_start', 'desktop.region_roi.end': 'region_end',
      'legacy.region_first': 'legacy_region_first', 'legacy.region_second': 'legacy_region_second',
    };
    const key = pointFields[field];
    if (key) {
      p.desktop.capture_points[key] = clone(point);
      const points = p.desktop.capture_points;
      if (key.startsWith('seat_') && points.seat_start && points.seat_end) p.vision.roi = rectangle(points.seat_start, points.seat_end);
      if (key.startsWith('region_') && points.region_start && points.region_end) p.desktop.region_roi = rectangle(points.region_start, points.region_end);
      if (points.legacy_region_first && points.legacy_region_second && p.desktop.cancellation_mode === 'legacyRegions') {
        p.legacy.cancellation_navigation = { type: 'two_regions', first: { type: 'point', point: points.legacy_region_first }, second: { type: 'point', point: points.legacy_region_second } };
      }
    } else if (field === 'desktop.stage_center') p.desktop.stage_center = clone(point);
    else if (['legacy.date', 'legacy.session', 'legacy.next', 'legacy.complete', 'refresh_target', 'entry_target'].includes(field)) setPath(p, field, { type: 'point', point: clone(point) });
    else throw new Error('지원하지 않는 좌표 항목입니다.');
  }
  if (frame) p.desktop.capture_reference = clone(frame);
  return p;
}

export function captureValue(profile, field) {
  const zone = regionCapture(field);
  if (zone?.edge) {
    const stored = profile.desktop?.capture_points?.[`zone${zone.index}_${zone.edge}`];
    if (stored) return stored;
    const roi = profile.legacy.regions[zone.index]?.vision?.roi;
    return roi ? { x: roi.x + (zone.edge === 'end' ? roi.width : 0), y: roi.y + (zone.edge === 'end' ? roi.height : 0) } : null;
  }
  const aliases = {
    'vision.roi.start': 'seat_start', 'vision.roi.end': 'seat_end',
    'desktop.region_roi.start': 'region_start', 'desktop.region_roi.end': 'region_end',
    'legacy.region_first': 'legacy_region_first', 'legacy.region_second': 'legacy_region_second',
  };
  if (aliases[field]) {
    const stored = profile.desktop?.capture_points?.[aliases[field]];
    if (stored) return stored;
    const navigation = profile.legacy?.cancellation_navigation;
    if (navigation?.type === 'two_regions' && field.startsWith('legacy.region_')) {
      return field === 'legacy.region_first' ? navigation.first : navigation.second;
    }
    const roi = field.startsWith('vision.roi') ? profile.vision?.roi : profile.desktop?.region_roi;
    return roi && field.includes('.roi.')
      ? { x: roi.x + (field.endsWith('.end') ? roi.width : 0), y: roi.y + (field.endsWith('.end') ? roi.height : 0) } : null;
  }
  return getPath(profile, field);
}

export function clearCapture(profile, field) {
  const p = mergeProfile(profile);
  const zone = regionCapture(field);
  if (zone) {
    const region = ensureLegacyRegion(p, zone.index);
    if (zone.edge) {
      delete p.desktop.capture_points[`zone${zone.index}_${zone.edge}`];
      region.vision = null;
    } else region[zone.kind] = null;
    return p;
  }
  const aliases = {
    'vision.roi.start': 'seat_start', 'vision.roi.end': 'seat_end',
    'desktop.region_roi.start': 'region_start', 'desktop.region_roi.end': 'region_end',
    'legacy.region_first': 'legacy_region_first', 'legacy.region_second': 'legacy_region_second',
  };
  if (aliases[field]) {
    delete p.desktop.capture_points[aliases[field]];
    if (field.startsWith('vision.roi')) p.vision.roi = null;
    if (field.startsWith('desktop.region_roi')) p.desktop.region_roi = null;
    if (field.startsWith('legacy.region')) p.legacy.cancellation_navigation = { type: 'none' };
  } else setPath(p, field, null);
  return p;
}

// Runtime revisions increase across runs. A delayed reply or bootstrap must
// not replace the state of a newer execution, including in the status overlay.
export function snapshotIsCurrent(incoming, current = {}) {
  const incomingRun = incoming.runId ?? incoming.run_id;
  const currentRun = current.runId ?? current.run_id;
  for (const [next, previous] of [[incoming.controlEpoch, current.controlEpoch],
    [incomingRun, currentRun], [incoming.revision, current.revision]]) {
    if (Number.isSafeInteger(next) && Number.isSafeInteger(previous) && next < previous) return false;
  }
  return !(current.phase === 'stopped' && incoming.phase !== 'stopped'
    && incomingRun === currentRun && incoming.revision === current.revision
    && incoming.controlEpoch === current.controlEpoch);
}

export function normalizeSnapshot(snapshot = {}) {
  const selectedIds = snapshot.selected_ids || snapshot.selectedIds || [];
  const attemptedIds = snapshot.attempted_ids || snapshot.attemptedIds || [];
  const mode = snapshot.mode || 'normal';
  const optimistic = (snapshot.selectionPolicy || (mode === 'seats' ? 'optimistic' : 'confirmed')) === 'optimistic';
  const selectedCount = snapshot.selectedCount ?? selectedIds.length;
  const estimatedCount = Number.isSafeInteger(snapshot.estimatedCount) && snapshot.estimatedCount >= 0
    ? snapshot.estimatedCount : new Set([...selectedIds, ...attemptedIds]).size;
  const countVerification = snapshot.selectionCountVerification === true;
  const siteCount = Number.isSafeInteger(snapshot.siteSelectionCount) && snapshot.siteSelectionCount >= 0
    ? snapshot.siteSelectionCount : null;
  const phase = snapshot.phase || 'idle';
  const manualEntryWaiting = phase === 'captcha' && snapshot.manualEntry === true
    && snapshot.nolCaptcha?.enabled !== true && snapshot.enterWatch?.enabled !== true;
  const waitingForSeatArea = snapshot.waitingForSeatArea === true;
  const recovery = targetRecoveryView(snapshot);
  const visual = visualDialogView({ ...snapshot, mode, phase, selectionPolicy: optimistic ? 'optimistic' : 'confirmed' });
  const keyboard = keyboardCheckView(snapshot);
  return {
    ...snapshot,
    phase,
    waitingForSeatArea,
    label: (keyboard.active ? `키보드 수신 확인 · ${keyboard.seconds}초` : keyboard.present ? keyboard.label : null) || recovery?.label || (waitingForSeatArea ? '좌석 영역 미지정 · 지정 대기' : null) || (visual?.active ? visual.label : null)
      || (manualEntryWaiting ? '날짜·회차 직접 진행 대기'
        : phase === 'completed' ? PHASE_LABELS.completed : snapshot.phaseLabel || PHASE_LABELS[phase]) || '상태 확인',
    message: (keyboard.active ? keyboard.message : null) || recovery?.message || (!waitingForSeatArea && visual?.active ? visual.message : null) || snapshot.message || '',
    mode,
    selectedIds,
    attemptedIds,
    selectedCount,
    selectionPolicy: optimistic ? 'optimistic' : 'confirmed',
    estimatedCount,
    displaySelectedCount: countVerification ? (siteCount ?? '—') : optimistic ? estimatedCount : selectedCount,
    selectionCountLabel: countVerification ? (snapshot.selectionCountPending ? '사이트 수량 확인 중' : '사이트 선택 수')
      : optimistic ? '선택 추정' : '확인된 선택',
    targetCount: snapshot.targetCount ?? snapshot.target_seats ?? 2,
    running: snapshot.running ?? !['idle', 'stopped', 'completed', 'faulted', 'unsupported'].includes(phase),
    paused: snapshot.paused ?? phase === 'paused',
    runId: snapshot.runId ?? snapshot.run_id,
    revision: snapshot.revision,
  };
}

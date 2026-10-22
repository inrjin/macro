// No transport, credentials, browser attachment, input dispatch or network requests.
const SEMANTICS = new Set([
  'booking', 'date', 'session', 'captcha', 'queue', 'seat_map',
  'region_map', 'sale_gate', 'countdown', 'completion', 'conflict',
]);
const BOOKING_NAMES = new Set(['예매하기', '예매']);
const documentRegistries = new WeakMap();

function text(value) {
  return typeof value === 'string' ? value.normalize('NFKC').replace(/\s+/g, ' ').trim() : '';
}

function explicitBoolean(value) {
  if (value === true || value === 'true') return true;
  if (value === false || value === 'false') return false;
  return undefined;
}

function fullDate(value) {
  const cleaned = text(value);
  const match = /^(\d{4})-(\d{2})-(\d{2})(?:T.*)?$/.exec(cleaned)
    || /^(\d{4})년\s*(\d{1,2})월\s*(\d{1,2})일(?:\s+.*)?$/.exec(cleaned);
  if (!match) return null;
  const [, year, month, day] = match.map(Number);
  const date = new Date(Date.UTC(year, month - 1, day));
  if (date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1 || date.getUTCDate() !== day) return null;
  return `${String(year).padStart(4, '0')}-${String(month).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
}

function fullTime(value) {
  const cleaned = text(value);
  const match = /^(?:\d{4}-\d{2}-\d{2}T)?(\d{1,2}):(\d{2})(?::00)?(?:Z|[+-]\d{2}:\d{2})?$/.exec(cleaned);
  if (!match || Number(match[1]) > 23 || Number(match[2]) > 59) return null;
  return `${match[1].padStart(2, '0')}:${match[2]}`;
}

/**
 * Normalization does not promote missing visibility/enabled evidence to true.
 * `semantic` comes from a runtime adapter contract, not an arbitrary text guess.
 */
export function normalizeControl(input) {
  if (!input || typeof input !== 'object') return null;
  const id = text(input.id);
  if (!id) return null;
  const attributes = input.attributes || {};
  const ariaLabel = text(input.ariaLabel ?? attributes['aria-label']);
  const accessibleName = text(input.accessibleName) || ariaLabel || text(input.text);
  const datetime = text(input.datetime);
  const date = fullDate(input.fullDate) || fullDate(input.dateLabel)
    || fullDate(ariaLabel) || fullDate(datetime);
  const session = fullTime(input.sessionTime) || fullTime(datetime);
  const semantic = SEMANTICS.has(input.semantic) ? input.semantic
    : session && input.role === 'button' ? 'session' : date ? 'date' : null;
  const explicitlyDisabled = explicitBoolean(input.disabled) === true
    || explicitBoolean(attributes['aria-disabled']) === true
    || Object.hasOwn(attributes, 'disabled');
  const selected = explicitBoolean(input.selected)
    ?? explicitBoolean(attributes['data-is-selected'])
    ?? explicitBoolean(attributes['aria-selected'])
    ?? explicitBoolean(attributes['aria-pressed'])
    ?? false;
  return {
    id,
    semantic,
    role: text(input.role),
    accessibleName,
    ariaLabel,
    datetime,
    fullDate: date,
    sessionTime: session,
    visible: input.visible === true && explicitBoolean(attributes['aria-hidden']) !== true,
    enabled: input.enabled === true && !explicitlyDisabled,
    selected,
    documentId: text(input.documentId),
    frameId: text(input.frameId) || 'main',
    source: text(input.source) || 'control_record',
  };
}

export function normalizeControls(records = []) {
  if (!Array.isArray(records)) throw new TypeError('controls must be an array');
  return records.map(normalizeControl).filter(Boolean);
}

function contextOf(observation) {
  const documentId = text(observation.documentId || observation.context?.documentId);
  const frameId = text(observation.frameId || observation.context?.frameId) || 'main';
  return { documentId, frameId, screenId: documentId ? `${documentId}/${frameId}` : '' };
}

function result(context, stage, control, reason, matchedValue = null, enabled) {
  return {
    event: {
      type: 'entry_observed',
      stage,
      target: control ? { type: 'element', id: control.id } : null,
      enabled: enabled ?? Boolean(control?.enabled),
      matched_value: matchedValue,
      screen_id: context.screenId,
    },
    evidence: control ? [{
      control_id: control.id,
      document_id: context.documentId,
      frame_id: context.frameId,
      source: control.source,
      accessible_name: control.accessibleName,
      semantic: control.semantic,
    }] : [],
    reason,
  };
}

function unknown(context, reason) {
  return result(context, 'unknown', null, reason, null, false);
}

function uniqueById(controls) {
  const ids = new Set();
  for (const control of controls) {
    if (ids.has(control.id)) return false;
    ids.add(control.id);
  }
  return true;
}

/**
 * Classifies positive, current DOM evidence only. No lack-of-error success rule.
 * Returns a diagnostic envelope; `classifyObservation` returns its core event.
 */
export function inspectObservation(observation = {}, profile = {}) {
  const context = contextOf(observation);
  if (!context.documentId) return unknown(context, 'A stable documentId is required; a URL is not a document identity.');
  const all = normalizeControls(observation.controls);
  const visible = all.filter((control) => control.visible);
  if (visible.some((control) => control.documentId && control.documentId !== context.documentId)) {
    return unknown(context, 'Controls from different documents must be observed separately.');
  }
  if (visible.some((control) => control.frameId !== context.frameId)) {
    return unknown(context, 'Controls from different frames must be observed separately.');
  }
  if (!uniqueById(visible)) return unknown(context, 'Duplicate control IDs make the current screen ambiguous.');
  const marked = (semantic) => visible.filter((control) => control.semantic === semantic);
  // CAPTCHA/queue presence wins over apparently clickable controls behind them.
  if (marked('captcha').length) return result(context, 'captcha', null, 'Visible CAPTCHA marker; manual input is required.', null, false);
  if (marked('queue').length) return result(context, 'queue', null, 'Visible waiting queue marker.', null, false);
  if (marked('conflict').length || marked('completion').length) {
    return unknown(context, 'Seat outcome markers require the separate explicit result protocol.');
  }
  const maps = [...marked('seat_map'), ...marked('region_map')];
  if (maps.length > 1) return unknown(context, 'More than one active map is visible.');
  if (maps.length === 1) return result(context, 'seat_map', null, `Visible ${maps[0].semantic} from the runtime contract.`, null, true);

  const dates = marked('date');
  const sessions = marked('session');
  const desiredDate = fullDate(profile.desired_date);
  const desiredSession = text(profile.desired_session);
  const desiredTime = fullTime(desiredSession);
  if (dates.length) {
    if (!desiredDate) return unknown(context, 'Visible calendar requires a full desired_date; a day number alone is insufficient.');
    const matching = dates.filter((control) => control.fullDate === desiredDate);
    if (matching.length !== 1) return unknown(context, matching.length ? 'The desired full date matches multiple visible controls.' : 'The desired full date is not visible.');
    if (!matching[0].selected) return result(context, 'calendar', matching[0], 'Exact full date match.', desiredDate);
  }
  if (sessions.length) {
    if (!desiredSession) return unknown(context, 'Visible session controls require desired_session.');
    const matching = sessions.filter((control) => {
      if (desiredDate && control.fullDate && control.fullDate !== desiredDate) return false;
      if (desiredTime) return control.sessionTime === desiredTime
        || (!control.sessionTime && control.accessibleName === desiredSession);
      return control.accessibleName === desiredSession;
    });
    if (matching.length !== 1) return unknown(context, matching.length ? 'The desired session matches multiple visible controls.' : 'The desired session is not visible.');
    if (!matching[0].selected) return result(context, 'session', matching[0], 'Exact session time/name match.', desiredSession);
  }

  const booking = visible.filter((control) => control.semantic === 'booking'
    || (!control.semantic && ['button', 'link'].includes(control.role) && BOOKING_NAMES.has(control.accessibleName)));
  if (booking.length !== 1) return unknown(context, booking.length ? 'More than one visible booking button; no target was chosen.' : 'No unambiguous visible entry control.');
  const gate = marked('sale_gate').length || marked('countdown').length;
  if (gate) return result(context, 'sale_gate', booking[0], 'Sale gate/countdown marker and a unique booking button.');
  if (dates.length || sessions.length || observation.pageKind === 'booking_confirmation') {
    return result(context, 'confirm_booking', booking[0], 'Date/session selection and a unique final booking control.');
  }
  return result(context, 'product_booking', booking[0], 'Unique visible booking control.');
}

export function classifyObservation(observation, profile) {
  return inspectObservation(observation, profile).event;
}

function isVisible(element, view) {
  if (element.hidden || element.closest?.('[hidden], [inert], [aria-hidden="true"]')) return false;
  const rect = element.getBoundingClientRect();
  const style = view?.getComputedStyle(element);
  return rect.width > 0 && rect.height > 0
    && (!style || (style.display !== 'none' && style.visibility !== 'hidden' && style.visibility !== 'collapse' && style.opacity !== '0'));
}

function isEnabled(element) {
  return !element.disabled && !element.matches?.(':disabled')
    && element.getAttribute('aria-disabled') !== 'true'
    && !element.closest?.('[inert]');
}

function stableId(document, element, context) {
  let registry = documentRegistries.get(document);
  if (!registry || registry.documentId !== context.documentId || registry.frameId !== context.frameId) {
    registry = { documentId: context.documentId, frameId: context.frameId, next: 0, elements: new WeakMap() };
    documentRegistries.set(document, registry);
  }
  if (!registry.elements.has(element)) {
    registry.elements.set(element, `${context.screenId}/control-${++registry.next}`);
  }
  return registry.elements.get(element);
}

/**
 * Collects the current document only. A transport must enumerate accessible
 * frames/new documents and supply a stable identity for each.
 * contract = { selectors: { captcha: '#verified-marker', seat_map: '...' } }
 * Selectors are site-adapter configuration, not prerequired user coordinates.
 */
export function collectControls(document, options = {}) {
  const context = contextOf(options);
  if (!context.documentId) throw new TypeError('collectControls requires documentId');
  const contract = options.contract || {};
  const elements = new Map();
  for (const control of document.querySelectorAll('button, [role="button"], a[href], input[type="button"], input[type="submit"]')) {
    elements.set(control, null);
  }
  for (const [semantic, selectors] of Object.entries(contract.selectors || {})) {
    if (!SEMANTICS.has(semantic)) throw new TypeError(`Unsupported semantic marker: ${semantic}`);
    for (const selector of Array.isArray(selectors) ? selectors : [selectors]) {
      if (typeof selector !== 'string' || !selector.trim()) throw new TypeError(`Invalid selector for ${semantic}`);
      for (const element of document.querySelectorAll(selector)) {
        const previous = elements.get(element);
        if (previous && previous !== semantic) throw new TypeError(`Conflicting semantics on one element: ${previous}, ${semantic}`);
        elements.set(element, semantic);
      }
    }
  }
  const view = document.defaultView;
  const controls = [];
  for (const [element, semantic] of elements) {
    const attributes = Object.fromEntries(['aria-label', 'aria-disabled', 'aria-selected', 'aria-pressed', 'aria-hidden', 'data-is-selected']
      .filter((name) => element.hasAttribute(name)).map((name) => [name, element.getAttribute(name)]));
    if (element.hasAttribute('disabled')) attributes.disabled = '';
    const dateLabel = element.querySelector('abbr[aria-label]')?.getAttribute('aria-label') || '';
    const datetime = element.getAttribute('datetime')
      || element.querySelector('time[datetime]')?.getAttribute('datetime') || '';
    const role = element.getAttribute('role') || (element.tagName === 'A' ? 'link' : element.tagName === 'BUTTON' || element.tagName === 'INPUT' ? 'button' : 'marker');
    const normalized = normalizeControl({
      id: stableId(document, element, context),
      semantic,
      role,
      accessibleName: element.getAttribute('aria-label') || element.innerText || element.textContent || '',
      dateLabel,
      datetime,
      visible: isVisible(element, view),
      enabled: isEnabled(element),
      attributes,
      documentId: context.documentId,
      frameId: context.frameId,
      source: semantic ? 'runtime_contract' : dateLabel ? 'full_date_accessibility' : datetime ? 'machine_datetime' : 'accessible_control',
    });
    const selectedSelectors = contract.selectedSelectors?.[normalized.semantic];
    if (selectedSelectors !== undefined) {
      const selectors = Array.isArray(selectedSelectors) ? selectedSelectors : [selectedSelectors];
      if (!selectors.every((selector) => typeof selector === 'string' && selector.trim())) {
        throw new TypeError(`Invalid selected selector for ${normalized.semantic}`);
      }
      normalized.selected = selectors.some((selector) => element.matches(selector));
    }
    controls.push(normalized);
  }
  return controls;
}

export function observeDocument(document, { profile = {}, ...options } = {}) {
  return inspectObservation({ ...options, controls: collectControls(document, options) }, profile);
}

import test from 'node:test';
import assert from 'node:assert/strict';
import { classifyObservation, collectControls, inspectObservation, normalizeControl } from '../adapters/browser/observer.js';

const profile = { desired_date: '2027-01-16', desired_session: '19:00' };
const control = (id, extra = {}) => ({ id, role: 'button', visible: true, enabled: true, ...extra });
const booking = (extra = {}) => control('book', { accessibleName: '예매하기', ...extra });
const observe = (controls, extra = {}) => classifyObservation({ documentId: 'doc-1', controls, ...extra }, profile);
const date = (extra = {}) => control('date-16', { dateLabel: '2027년 1월 16일', ...extra });
const session = (extra = {}) => control('session-19', { datetime: '2027-01-16T19:00:00+09:00', ...extra });

test('product booking returns an element reference without performing input', () => {
  const event = observe([booking()]);
  assert.deepEqual(event, {
    type: 'entry_observed', stage: 'product_booking', target: { type: 'element', id: 'book' },
    enabled: true, matched_value: null, screen_id: 'doc-1/main',
  });
});

test('sale gate remains disabled while a countdown is visible, then exposes enabled state', () => {
  const countdown = control('timer', { semantic: 'countdown', role: 'marker' });
  assert.equal(observe([booking({ enabled: false }), countdown]).stage, 'sale_gate');
  assert.equal(observe([booking({ enabled: false }), countdown]).enabled, false);
  assert.equal(observe([booking(), countdown]).enabled, true);
});

test('full calendar date precedes session selection, then final booking', () => {
  const first = observe([booking(), date(), session()]);
  assert.equal(first.stage, 'calendar');
  assert.equal(first.matched_value, '2027-01-16');
  const second = observe([booking(), date({ selected: true }), session()]);
  assert.equal(second.stage, 'session');
  assert.equal(second.matched_value, '19:00');
  const third = observe([booking(), date({ selected: true }), session({ selected: true })]);
  assert.equal(third.stage, 'confirm_booking');
});

test('CAPTCHA and queue take precedence over booking buttons and seat maps behind them', () => {
  const map = control('map', { semantic: 'seat_map' });
  const captcha = control('captcha', { semantic: 'captcha', enabled: false });
  const queue = control('queue', { semantic: 'queue' });
  assert.equal(observe([booking(), map, captcha, queue]).stage, 'captcha');
  assert.equal(observe([booking(), map, queue]).stage, 'queue');
  assert.equal(observe([booking(), map]).stage, 'seat_map');
  assert.equal(observe([booking(), map]).target, null);
});

test('day numbers, wrong month, and impossible full dates cannot select a date', () => {
  const dayOnly = control('day-16', { semantic: 'date', accessibleName: '16' });
  assert.equal(observe([dayOnly, booking()]).stage, 'unknown');
  assert.equal(observe([date({ dateLabel: '2027년 2월 16일' }), booking()]).stage, 'unknown');
  assert.equal(normalizeControl(date({ dateLabel: '2027년 2월 30일' })).fullDate, null);
});

test('exact full date resolves identical day numbers in adjacent months', () => {
  const selected = observe([
    date({ id: 'other-month', dateLabel: '2027년 2월 16일' }),
    date(), booking(),
  ]);
  assert.deepEqual(selected.target, { type: 'element', id: 'date-16' });
});

test('duplicate visible buttons and duplicate matching dates/sessions are ambiguous', () => {
  assert.equal(observe([booking(), booking({ id: 'book-two' })]).stage, 'unknown');
  assert.equal(observe([date(), date({ id: 'other-date' })]).stage, 'unknown');
  assert.equal(observe([session(), session({ id: 'other-session' })]).stage, 'unknown');
});

test('visibility and availability require positive evidence and respect aria-disabled', () => {
  assert.equal(observe([booking({ visible: undefined })]).stage, 'unknown');
  assert.equal(observe([booking({ enabled: undefined })]).enabled, false);
  assert.equal(observe([booking({ attributes: { 'aria-disabled': 'true' } })]).enabled, false);
  assert.equal(observe([booking({ attributes: { 'aria-hidden': 'true' } })]).stage, 'unknown');
});

test('same URL in different windows/documents retains distinct observation identities', () => {
  const first = observe([booking()], { documentId: 'window-1/nav-3', url: 'https://example.test/ticket' });
  const second = observe([booking()], { documentId: 'window-2/nav-1', url: 'https://example.test/ticket' });
  assert.notEqual(first.screen_id, second.screen_id);
  assert.equal(observe([booking()], { documentId: '' }).stage, 'unknown');
});

test('mixed document/frame evidence and reused control IDs are rejected', () => {
  assert.equal(observe([booking({ documentId: 'other' })]).stage, 'unknown');
  assert.equal(observe([booking({ frameId: 'seats' })]).stage, 'unknown');
  assert.equal(observe([booking(), control('book', { semantic: 'countdown' })]).stage, 'unknown');
  assert.equal(observe([booking({ frameId: 'seats' })], { frameId: 'seats' }).stage, 'product_booking');
});

test('error absence, stale completion markers and booking-history text never imply success', () => {
  assert.equal(observe([]).stage, 'unknown');
  assert.equal(observe([control('done', { semantic: 'completion' })]).stage, 'unknown');
  assert.equal(observe([control('history', { accessibleName: '예매 내역' })]).stage, 'unknown');
  assert.equal(observe([control('cancel', { accessibleName: '예매 취소' })]).stage, 'unknown');
});

test('diagnostic evidence identifies the positive control without adding core event fields', () => {
  const inspected = inspectObservation({ documentId: 'doc-1', controls: [booking()] }, profile);
  assert.equal(inspected.evidence[0].control_id, 'book');
  assert.match(inspected.reason, /Unique visible/);
  assert.deepEqual(Object.keys(inspected.event).sort(), ['enabled', 'matched_value', 'screen_id', 'stage', 'target', 'type']);
});

test('same-time sessions on a different full date are not interchangeable', () => {
  assert.equal(observe([session({ datetime: '2027-01-17T19:00:00+09:00' })]).stage, 'unknown');
});

test('collector retains control identities and supports explicitly verified selected-state selectors', () => {
  const attributes = new Map();
  const button = {
    tagName: 'BUTTON', disabled: false, hidden: false, innerText: '16',
    get value() { throw new Error('Input values must never be read.'); },
    getAttribute: (name) => attributes.get(name) ?? null,
    hasAttribute: (name) => attributes.has(name),
    closest: () => null,
    getBoundingClientRect: () => ({ width: 40, height: 40 }),
    matches: (selector) => selector === '.fixture-selected',
    querySelector: (selector) => selector === 'abbr[aria-label]' ? { getAttribute: () => '2027년 1월 16일' } : null,
  };
  const document = {
    defaultView: { getComputedStyle: () => ({ display: 'block', visibility: 'visible', opacity: '1' }) },
    querySelectorAll: () => [button],
  };
  const options = { documentId: 'fixture-nav', contract: { selectedSelectors: { date: '.fixture-selected' } } };
  const first = collectControls(document, options)[0];
  const second = collectControls(document, options)[0];
  assert.equal(first.id, second.id);
  assert.equal(first.fullDate, '2027-01-16');
  assert.equal(first.selected, true);
  assert.equal(first.visible, true);
  const afterNavigation = collectControls(document, { ...options, documentId: 'fixture-new-nav' })[0];
  assert.notEqual(afterNavigation.id, first.id);
});

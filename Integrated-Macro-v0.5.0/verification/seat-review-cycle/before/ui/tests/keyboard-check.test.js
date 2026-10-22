// Executes fixture listeners with a minimal DOM. This does not test a browser,
// trusted OS events, screen pixels, AppKit monitors, or macro input delivery.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

const html = await readFile(new URL('../../fixtures/keyboard-check.html', import.meta.url), 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];

function fixture() {
  const elements = new Map([...html.matchAll(/id="([^"]+)"/g)].map(([, id]) => [id, {
    value: id === 'theme' ? 'brighten' : '', hidden: id === 'backdrop', disabled: false,
    textContent: '', listeners: {}, focus() {},
    addEventListener(type, callback) { this.listeners[type] = callback; },
  }]));
  const body = { dataset: { theme: 'brighten' } };
  let now = 1000;
  vm.runInNewContext(script, {
    document: { body, getElementById: (id) => elements.get(id) },
    Date: { now: () => now },
  });
  return {
    body, elements, setTime(value) { now = value; },
    dispatch(id, type, event = {}) { elements.get(id).listeners[type](event); },
    state() { return JSON.parse(elements.get('metadata').textContent); },
  };
}

function metadataOnlyEvent(isTrusted = true) {
  const event = { isTrusted };
  for (const field of ['key', 'code', 'data', 'keyCode', 'which']) {
    Object.defineProperty(event, field, { get() { throw new Error(`Fixture read input content: ${field}`); } });
  }
  return event;
}

test('keyboard fixture keeps the modal and backdrop open through first and later input', () => {
  const page = fixture();
  page.dispatch('open-input', 'click');
  page.setTime(1010);
  page.dispatch('sample-input', 'keydown', metadataOnlyEvent());
  page.dispatch('sample-input', 'input', metadataOnlyEvent());
  assert.equal(page.state().modalOpen, true);
  assert.equal(page.elements.get('backdrop').hidden, false);
  assert.equal(page.state().firstKeyAtMs, 1010);
  page.setTime(1020);
  page.dispatch('sample-input', 'keydown', metadataOnlyEvent(false));
  page.dispatch('sample-input', 'input', metadataOnlyEvent(false));
  assert.equal(page.state().firstKeyAtMs, 1010);
  assert.equal(page.state().lastKeyAtMs, 1020);
  assert.equal(page.state().keydownCount, 2);
  assert.equal(page.state().trustedKeydownCount, 1);
  assert.equal(page.state().inputEventCount, 2);
  assert.equal(page.state().trustedInputEventCount, 1);
  assert.equal(page.state().submitCount, 0);
  assert.equal(page.state().closedAtMs, null);
});

test('keyboard fixture records explicit submission separately and counts only subsequent target clicks', () => {
  const page = fixture();
  page.dispatch('continue-target', 'click');
  page.dispatch('open-input', 'click');
  page.dispatch('continue-target', 'click');
  assert.equal(page.state().continuationClicks, 0);
  page.setTime(1040);
  let prevented = false;
  const submit = { preventDefault() { prevented = true; } };
  page.dispatch('input-form', 'submit', submit);
  assert.equal(prevented, true);
  assert.equal(page.elements.get('backdrop').hidden, true);
  assert.equal(page.state().modalOpen, false);
  assert.equal(page.state().closedAtMs, 1040);
  assert.equal(page.state().submitCount, 1);
  page.dispatch('input-form', 'submit', submit);
  page.dispatch('sample-input', 'keydown', metadataOnlyEvent());
  assert.equal(page.state().submitCount, 1);
  assert.equal(page.state().keydownCount, 0);
  page.dispatch('continue-target', 'click');
  assert.equal(page.state().continuationClicks, 1);
});

test('keyboard fixture supports both background directions and discards previous run metadata', () => {
  const page = fixture();
  page.elements.get('theme').value = 'darken';
  page.dispatch('theme', 'change');
  page.dispatch('open-input', 'click');
  assert.equal(page.body.dataset.theme, 'darken');
  assert.equal(page.state().theme, 'darken');
  assert.equal(page.elements.get('theme').disabled, true);
  page.dispatch('sample-input', 'keydown', metadataOnlyEvent());
  page.dispatch('input-form', 'submit', { preventDefault() {} });
  assert.equal(page.elements.get('theme').disabled, false);
  page.elements.get('theme').value = 'brighten';
  page.dispatch('theme', 'change');
  page.setTime(2000);
  page.dispatch('open-input', 'click');
  assert.equal(page.body.dataset.theme, 'brighten');
  assert.equal(page.state().theme, 'brighten');
  assert.equal(page.state().openedAtMs, 2000);
  assert.equal(page.state().keydownCount, 0);
  assert.equal(page.state().firstKeyAtMs, null);
  assert.equal(page.state().closedAtMs, null);
  assert.equal(page.state().submitCount, 0);
});

test('keyboard fixture serializes only counts, timestamps, and explicit display state', () => {
  const page = fixture();
  page.dispatch('open-input', 'click');
  Object.defineProperty(page.elements.get('sample-input'), 'value', {
    get() { throw new Error('Fixture read input value'); }, set() {}, configurable: true,
  });
  page.dispatch('sample-input', 'keydown', metadataOnlyEvent());
  page.dispatch('sample-input', 'input', metadataOnlyEvent());
  page.dispatch('input-form', 'submit', { preventDefault() {} });
  assert.deepEqual(Object.keys(page.state()).sort(), [
    'closedAtMs', 'continuationClicks', 'firstKeyAtMs', 'inputEventCount',
    'keydownCount', 'lastKeyAtMs', 'modalOpen', 'openedAtMs', 'submitCount',
    'theme', 'trustedInputEventCount', 'trustedKeydownCount',
  ].sort());
});

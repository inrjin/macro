// Minimal DOM for executing application listeners without a browser or network.
// It intentionally does not emulate CSS, layout, focus, or native input.
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

class Element {
  constructor(tagName) {
    this.tagName = tagName;
    this.children = [];
    this.dataset = {};
    this.attributes = {};
    this.style = {};
    this.listeners = {};
    this.classes = new Set();
    this.value = '';
    this.checked = false;
    this.hidden = false;
    this.disabled = false;
    this.classList = {
      contains: (name) => this.classes.has(name),
      toggle: (name, force = !this.classes.has(name)) => {
        if (force) this.classes.add(name);
        else this.classes.delete(name);
        return force;
      },
      add: (name) => this.classes.add(name),
      remove: (name) => this.classes.delete(name),
    };
  }
  set className(value) { this.classes = new Set(value.split(/\s+/).filter(Boolean)); }
  get className() { return [...this.classes].join(' '); }
  set textContent(value) { this.children = []; this.text = String(value); }
  get textContent() { return (this.text || '') + this.children.map((child) => child.textContent).join(''); }
  get options() { return this.children.filter((child) => child.tagName === 'option'); }
  get firstElementChild() { return this.children.find((child) => child.tagName !== '#text'); }
  get lastElementChild() { return this.children.findLast((child) => child.tagName !== '#text'); }
  append(...children) { for (const child of children) { child.parent = this; this.children.push(child); } }
  prepend(child) { child.parent = this; this.children.unshift(child); }
  after(child) {
    child.parent = this.parent;
    this.parent.children.splice(this.parent.children.indexOf(this) + 1, 0, child);
  }
  remove() { this.parent.children.splice(this.parent.children.indexOf(this), 1); }
  replaceChildren(...children) { this.text = ''; this.children = []; this.append(...children); }
  setAttribute(name, value) {
    this.attributes[name] = value;
    if (name.startsWith('data-')) this.dataset[name.slice(5).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase())] = value;
    else if (name === 'class') this.className = value;
    else if (['hidden', 'checked', 'disabled'].includes(name)) this[name] = true;
    else this[name] = value;
  }
  removeAttribute(name) { delete this.attributes[name]; }
  addEventListener(name, callback) { (this.listeners[name] ||= []).push(callback); }
  async click() { if (!this.disabled) for (const callback of this.listeners.click || []) await callback({ target: this }); }
  matches(selector) {
    const checked = selector.endsWith(':checked');
    if (checked) selector = selector.slice(0, -8);
    if (checked && !this.checked) return false;
    const attribute = selector.match(/\[([^\]^=]+)(\^?=)?"?([^\]"]*)"?\]$/);
    if (attribute) {
      selector = selector.slice(0, attribute.index);
      const [, name, operator, expected] = attribute;
      const actual = name.startsWith('data-')
        ? this.dataset[name.slice(5).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase())]
        : this[name] ?? this.attributes[name];
      if (actual === undefined) return false;
      if (operator === '=' && actual !== expected) return false;
      if (operator === '^=' && !String(actual).startsWith(expected)) return false;
    }
    if (!selector) return true;
    if (selector.startsWith('#')) return this.id === selector.slice(1);
    if (selector.startsWith('.')) return this.classes.has(selector.slice(1));
    return this.tagName === selector;
  }
  querySelectorAll(selector) {
    const selectors = selector.split(',').map((part) => part.trim());
    const matches = [];
    const visit = (element) => {
      for (const child of element.children) {
        if (selectors.some((part) => child.matches(part))) matches.push(child);
        visit(child);
      }
    };
    visit(this);
    return matches;
  }
  querySelector(selector) { return this.querySelectorAll(selector)[0] || null; }
}

function parseHtml(html) {
  const document = new Element('#document');
  const stack = [document];
  const voidTags = new Set(['meta', 'link', 'input', 'br', 'img', 'hr']);
  for (const token of html.match(/<[^>]*>|[^<]+/g) || []) {
    if (token.startsWith('<!')) continue;
    if (token.startsWith('</')) { stack.pop(); continue; }
    if (token.startsWith('<')) {
      const [, tag, attrs] = token.match(/^<([^\s>]+)([^>]*)>/);
      const element = new Element(tag);
      for (const attribute of attrs.matchAll(/([^\s=]+)(?:="([^"]*)")?/g)) element.setAttribute(attribute[1], attribute[2] ?? '');
      stack.at(-1).append(element);
      if (!voidTags.has(tag)) stack.push(element);
    } else {
      const text = new Element('#text');
      text.textContent = token;
      stack.at(-1).append(text);
    }
  }
  document.createElement = (tag) => new Element(tag);
  document.createTextNode = (text) => { const node = new Element('#text'); node.textContent = text; return node; };
  document.getElementById = (id) => document.querySelector(`#${id}`);
  return document;
}

export async function loadAppPage(htmlUrl, bootstrap, invokeExtra, bootstrapProfile, options = {}) {
  const document = parseHtml(await readFile(htmlUrl, 'utf8'));
  const listeners = {};
  const calls = [];
  let current = bootstrap;
  const emit = (name, payload) => {
    if (name === 'state-change') current = payload;
    for (const callback of listeners[name] || []) callback({ payload });
  };
  const window = {
    addEventListener() {}, scrollTo() {},
    __TAURI__: {
      event: { listen: async (name, callback) => { (listeners[name] ||= []).push(callback); return () => {}; } },
      core: { invoke: async (name, args) => {
        calls.push({ name, args });
        if (name === 'get_bootstrap') {
          if (options.bootstrapWait) await options.bootstrapWait;
          if (options.bootstrapProvider) return options.bootstrapProvider(current);
          return { snapshot: current, windows: [], checks: [], platform: 'DOM fixture', profile: bootstrapProfile, profileRevision: options.profileRevision ?? 0 };
        }
        return invokeExtra(name, args, emit);
      } },
    },
  };
  const context = vm.createContext({ document, window, console, structuredClone, URL, setTimeout, clearTimeout, navigator: { platform: 'DOM fixture' } });
  const cache = new Map();
  async function load(url) {
    if (cache.has(url.href)) return cache.get(url.href);
    const module = new vm.SourceTextModule(await readFile(url, 'utf8'), { context, identifier: url.href });
    cache.set(url.href, module);
    await module.link((specifier) => load(new URL(specifier, url)));
    return module;
  }
  const script = document.querySelector('script').src;
  const module = await load(new URL(script, htmlUrl));
  await module.evaluate();
  // initialize() attaches listeners and invokes bootstrap asynchronously.
  await new Promise((resolve) => setTimeout(resolve, 0));
  return { document, emit, calls, listeners };
}

// Exercises the shipped HTML and JS in Chrome; Tauri/native I/O is replaced by
// explicit event fixtures. It does not capture a real window or synthesize input.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { once } from 'node:events';

const uiRoot = path.resolve(fileURLToPath(new URL('../', import.meta.url)));
const browserPath = process.env.CHROME_PATH || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const profilePath = await mkdtemp(path.join(tmpdir(), 'macro-ui-check-'));
const server = createServer(async (request, response) => {
  try {
    const name = new URL(request.url, 'http://localhost').pathname;
    const file = path.resolve(uiRoot, `.${name}`);
    if (!file.startsWith(`${uiRoot}${path.sep}`)) throw new Error('Invalid file');
    const type = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css' }[path.extname(file)] || 'application/octet-stream';
    response.writeHead(200, { 'content-type': type });
    response.end(await readFile(file));
  } catch {
    response.writeHead(404);
    response.end();
  }
});
let browser;
let socket;
try {
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const origin = `http://127.0.0.1:${server.address().port}`;
  browser = spawn(browserPath, [
    '--headless', '--no-first-run', '--no-default-browser-check', '--disable-background-networking',
    '--disable-extensions', '--remote-debugging-port=0', `--user-data-dir=${profilePath}`, 'about:blank',
  ], { stdio: ['ignore', 'ignore', 'pipe'] });
  const debugUrl = await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Chrome DevTools startup timed out')), 15000);
    let output = '';
    browser.stderr.on('data', (chunk) => {
      output += chunk;
      const match = output.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match) { clearTimeout(timeout); resolve(match[1]); }
    });
    browser.once('error', (error) => { clearTimeout(timeout); reject(error); });
    browser.once('exit', (code, signal) => {
      clearTimeout(timeout);
      reject(new Error(`Chrome exited before DevTools: code=${code}, signal=${signal}\n${output.slice(-1500)}`));
    });
  });
  socket = new WebSocket(debugUrl);
  await once(socket, 'open');
  let nextId = 0;
  const pending = new Map();
  const browserErrors = [];
  socket.addEventListener('message', ({ data }) => {
    const message = JSON.parse(data);
    if (message.method === 'Runtime.exceptionThrown') browserErrors.push(message.params.exceptionDetails);
    const waiter = pending.get(message.id);
    if (!waiter) return;
    pending.delete(message.id);
    clearTimeout(waiter.timeout);
    if (message.error) waiter.reject(new Error(JSON.stringify(message.error)));
    else waiter.resolve(message.result);
  });
  const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
    const id = ++nextId;
    const timeout = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timed out: ${method}`)); }, 10000);
    pending.set(id, { resolve, reject, timeout });
    socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
  const evaluate = async (sessionId, expression) => {
    const result = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId);
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  const bootstrap = {
    phase: 'idle', runId: 1, revision: 0, captureState: 'idle', captureField: '', captureArmed: false,
    captureError: null, message: '', checks: [], manualActions: [], selectedCount: 0, targetCount: 2,
  };
  const fixture = `
    window.__test = {
      listeners: {}, snapshot: ${JSON.stringify(bootstrap)}, calls: [],
      emit(name, payload) {
        if (name === 'state-change') this.snapshot = payload;
        for (const listener of this.listeners[name] || []) listener({payload});
      },
    };
    window.__TAURI__ = {
      core: { invoke: async (name, args) => {
        __test.calls.push({name,args});
        if (name === 'get_bootstrap') return {snapshot:__test.snapshot,windows:[],checks:[],platform:'Browser fixture'};
        if (name === 'arm_capture') {
          const state = {...__test.snapshot,captureState:'armed',captureArmed:true,captureField:args.field,captureError:null,revision:1,message:'회차 버튼 · 예매창의 대상 위에서 F8'};
          __test.emit('state-change',state);
          return {field:args.field,profile:args.profile,snapshot:state};
        }
        throw new Error('Unexpected native command: '+name);
      }},
      event: {listen: async (name, callback) => {
        (__test.listeners[name] ||= []).push(callback);
        return () => {};
      }},
    };
  `;
  const openPage = async (file) => {
    const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
    await send('Runtime.enable', {}, sessionId);
    await send('Page.enable', {}, sessionId);
    await send('Page.addScriptToEvaluateOnNewDocument', { source: fixture }, sessionId);
    await send('Page.navigate', { url: `${origin}/${file}` }, sessionId);
    await evaluate(sessionId, `new Promise((resolve,reject) => {
      const began=Date.now(); const timer=setInterval(() => {
        if(window.__test?.listeners['state-change']?.length && document.readyState==='complete') {clearInterval(timer);resolve(true);}
        else if(Date.now()-began>8000) {clearInterval(timer);reject(new Error('UI initialization timed out'));}
      },20);
    })`);
    return sessionId;
  };
  const main = await openPage('index.html');
  const overlay = await openPage('overlay.html');
  const emit = async (name, payload) => {
    await Promise.all([main, overlay].map((session) => evaluate(session, `window.__test.emit(${JSON.stringify(name)},${JSON.stringify(payload)})`)));
  };
  const state = (revision, captureState, message, captureError = null) => ({
    ...bootstrap, revision, captureField: 'legacy.session', captureState, message, captureError,
    captureArmed: !['idle', 'captured'].includes(captureState),
  });
  await evaluate(main, `document.querySelector('[data-capture-field="legacy.session"]').click()`);
  await evaluate(main, `new Promise(resolve => setTimeout(resolve,0))`);
  const armCall = await evaluate(main, `window.__test.calls.find(call => call.name==='arm_capture')`);
  assert.equal(armCall.args.field, 'legacy.session');
  await emit('state-change', state(1, 'armed', '회차 버튼 · 예매창의 대상 위에서 F8'));
  await emit('state-change', state(2, 'reading', 'F8 수신 · 회차 버튼 읽는 중'));
  await emit('state-change', state(3, 'reading', '회차 버튼 · 예매창 화면 읽는 중'));
  const message = '[target_not_foreground] 연결한 예매창이 맨 앞에 있지 않습니다.';
  await emit('runtime-error', { message, captureField: 'legacy.session' });
  await emit('state-change', state(4, 'error', message, message));
  const failed = await evaluate(main, `({
    activity:document.querySelector('#activity-log').firstElementChild.textContent,
    activityError:document.querySelector('#activity-log').firstElementChild.classList.contains('error'),
    banner:document.querySelector('#capture-guide-title').textContent,
    detail:document.querySelector('#capture-guide-message').textContent,
    runbar:document.querySelector('#runbar-status').textContent,
    rowError:document.querySelector('[data-coordinate-field="legacy.session"]').classList.contains('capture-error'),
    output:document.querySelector('[data-point-field="legacy.session"]').textContent,
    feedbackHidden:document.querySelector('#feedback').hidden,
  })`);
  assert.match(failed.activity, /회차 버튼 지정 실패.*target_not_foreground/);
  assert.equal(failed.activityError, true);
  assert.match(failed.banner, /회차 버튼 · 지정 실패/);
  assert.match(failed.detail, /target_not_foreground.*F8/);
  assert.match(failed.runbar, /지정 실패/);
  assert.equal(failed.rowError, true);
  assert.equal(failed.output, '미지정');
  assert.equal(failed.feedbackHidden, false);
  const failedOverlay = await evaluate(overlay, `({
    error:document.querySelector('#overlay').classList.contains('error'),
    phase:document.querySelector('#overlay-phase').textContent,
    message:document.querySelector('#overlay-message').textContent,
  })`);
  assert.equal(failedOverlay.error, true);
  assert.match(failedOverlay.phase, /지정 실패.*target_not_foreground/);
  assert.match(failedOverlay.message, /F8/);
  await emit('state-change', state(5, 'reading', 'F8 수신 · 회차 버튼 읽는 중'));
  assert.equal(await evaluate(main, `document.querySelector('#feedback').hidden`), true);
  await emit('point-captured', {
    field: 'legacy.session', point: { x: 240, y: 360 }, color: { r: 10, g: 20, b: 30 },
    frame: { width: 1200, height: 800, scale_x: 2, scale_y: 2 },
  });
  await emit('state-change', state(6, 'captured', '회차 버튼 지정 완료'));
  const captured = await evaluate(main, `({
    output:document.querySelector('[data-point-field="legacy.session"]').textContent,
    saved:document.querySelector('#save-status').textContent,
    banner:document.querySelector('#capture-guide-title').textContent,
    rowError:document.querySelector('[data-coordinate-field="legacy.session"]').classList.contains('capture-error'),
    feedbackHidden:document.querySelector('#feedback').hidden,
  })`);
  assert.match(captured.output, /240.*360/);
  assert.match(captured.saved, /저장됨/);
  assert.match(captured.banner, /지정 완료/);
  assert.equal(captured.rowError, false);
  assert.equal(captured.feedbackHidden, true);
  assert.equal(await evaluate(overlay, `document.querySelector('#overlay').classList.contains('error')`), false);
  assert.deepEqual(browserErrors, []);
  console.log('PASS: Actual app.js and overlay.js in Chrome replay F8 arm → read → foreground failure → persistent failure → retry → saved coordinate.');
  console.log('Native Tauri I/O was stubbed; this does not verify macOS capture, permissions, or mouse input.');
} finally {
  socket?.close();
  if (browser && browser.exitCode === null) {
    browser.kill('SIGTERM');
    await Promise.race([once(browser, 'exit'), new Promise((resolve) => setTimeout(resolve, 2000))]);
    if (browser.exitCode === null) browser.kill('SIGKILL');
  }
  await new Promise((resolve) => server.close(resolve));
  await rm(profilePath, { recursive: true, force: true });
}

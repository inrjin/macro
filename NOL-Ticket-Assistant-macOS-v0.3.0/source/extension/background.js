import {allowedUrl, samePage, validateConfig, parseKst, clockSample, nextPhase} from './core.js';
import {SeatEngine, seatUrl} from './seat-engine.js';
import {validateSeatProfile} from './seat-core.js';
import {macLink} from './mac-link.js';
const HOST = 'com.nol_ticket_assistant.bridge';
let serial = Promise.resolve();
let nativePort = null, nativePending = null;
const locked = fn => { const result = serial.then(fn); serial = result.catch(() => {}); return result; };
const read = async () => (await chrome.storage.session.get('run')).run;
function closeNative() { const port = nativePort; nativePort = null; port?.disconnect(); }
function nativeExchange(message) {
  if (nativePending) return Promise.reject(new Error('Windows 연결 요청이 이미 진행 중입니다.'));
  if (!nativePort) {
    const port = chrome.runtime.connectNative(HOST);
    nativePort = port;
    port.onMessage.addListener(reply => {
      if (nativePending) { const request = nativePending; nativePending = null; clearTimeout(request.timer); request.resolve(reply); }
    });
    port.onDisconnect.addListener(() => {
      const error = chrome.runtime.lastError?.message || 'Windows 연결이 종료되었습니다.';
      if (nativePending) { const request = nativePending; nativePending = null; clearTimeout(request.timer); request.reject(new Error(error)); }
      if (nativePort === port) { nativePort = null; locked(async () => { const run = await read(); if (run?.config.autoMacro && ['ARMED','WAIT_READY'].includes(run.phase)) await stop(error); }); }
    });
  }
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { nativePending = null; closeNative(); reject(new Error('Windows 연결 응답 시간 초과. 자동 재시도하지 않습니다.')); }, 2500);
    nativePending = {resolve, reject, timer};
    try { nativePort.postMessage(message); } catch (e) { clearTimeout(timer); nativePending = null; closeNative(); reject(e); }
  });
}
async function save(run) { await chrome.storage.session.set({run}); }
async function record(message) {
  const {logs = []} = await chrome.storage.local.get('logs');
  logs.push({time: new Date().toISOString(), message});
  await chrome.storage.local.set({logs: logs.slice(-60)});
}
const seats = new SeatEngine(async status => {
  await chrome.storage.session.set({seatRun:status});
  if(['DONE','STOPPED'].includes(status?.phase)) await record(status.message);
  await chrome.action.setBadgeText({text:status?.phase==='WAIT_OUTCOME'?'좌석':status?.phase==='DONE'?'완료':''});
  mac.push();
});
const getStatus=async()=>({run:await read(),seatRun:seats.status(),...await chrome.storage.local.get(['config','clock','logs','profile']),extensionId:chrome.runtime.id});
const mac=macLink(getStatus,m=>locked(()=>handle(m,{url:chrome.runtime.getURL('native-control')})),record);
const tell = (id, m, options) => chrome.tabs.sendMessage(id, m, options).catch(() => null);
async function stop(message, state = 'STOPPED') {
  const run = await read();
  if (run) {
    run.phase = state; await save(run);
    await chrome.alarms.clear('expiry');
    await tell(run.tabId, {type: 'clear'});
    const tabs = await chrome.tabs.query({});
    await Promise.all(tabs.filter(t => t.openerTabId === run.tabId).map(t => tell(t.id, {type: 'clear'})));
    if (run.attached) { await chrome.debugger.detach({tabId: run.tabId}).catch(() => {}); run.attached = false; await save(run); }
  }
  closeNative();
  await record(message);
  await chrome.action.setBadgeText({text: state === 'DONE' ? '완료' : ''});
}
async function activeTab(tabId) {
  const tab = await chrome.tabs.get(tabId), win = await chrome.windows.get(tab.windowId);
  return tab.active && win.focused;
}
async function contextFor(sender, run) {
  if (!run || Date.now() >= run.expiresAt || !sender.tab || !allowedUrl(sender.url)) return null;
  if (run.phase === 'ARMED') {
    return sender.tab.id === run.tabId && sender.frameId === 0 && samePage(sender.url, run.config.url) ? run : null;
  }
  if (run.phase !== 'WAIT_READY' || !run.config.autoMacro || !samePage(sender.url, run.config.readyUrl)) return null;
  const tab = await chrome.tabs.get(sender.tab.id);
  return tab.id === run.tabId || tab.openerTabId === run.tabId ? run : null;
}
async function arm(config, tabId) {
  if(seats.busy())throw new Error('좌석 작업을 먼저 중지하세요.');
  validateConfig(config);
  const tab = await chrome.tabs.get(tabId);
  if (!samePage(tab.url, config.url)) throw new Error('현재 탭이 입력한 상품 페이지가 아닙니다.');
  const {run: prior} = await chrome.storage.session.get('run');
  if (prior && ['ARMED','WAIT_READY','STARTING'].includes(prior.phase)) throw new Error('이미 대기 중입니다. 먼저 중지하세요.');
  const sale = parseKst(config.saleAt);
  const run = {id: crypto.randomUUID(), tabId, config, phase: 'ARMED', attached: false,
    notBefore: sale === null ? 0 : sale, expiresAt: Date.now() + config.timeoutMinutes * 60000};
  if (run.notBefore >= run.expiresAt) throw new Error('오픈 시각이 최대 대기 시간보다 늦습니다.');
  try {
    if (config.autoMacro && config.seatDriver==='browser') validateSeatProfile((await chrome.storage.local.get('profile')).profile);
    if (config.autoMacro && config.seatDriver!=='browser') {
      const bridge = await nativeExchange({op: 'ping'});
      if (!bridge?.ok || !bridge.macroRunning) throw new Error(bridge?.detail || 'Windows 매크로를 먼저 실행하세요.');
    }
    await chrome.debugger.attach({tabId}, '1.3'); run.attached = true;
    await save(run);
    await chrome.alarms.create('expiry', {when: run.expiresAt});
    const reply = await chrome.tabs.sendMessage(tabId, {type: 'monitor', session: run}, {frameId: 0});
    if (!reply?.ok) throw new Error('페이지 감지기에 연결되지 않았습니다. 페이지를 새로고침하세요.');
    await record('대기 시작: 실제 예매 버튼의 표시·활성 상태를 기다립니다.');
    await chrome.action.setBadgeText({text: '대기'});
    return {ok: true};
  } catch (e) { await stop('대기 시작 실패: ' + e.message); throw e; }
}
async function handle(m, sender) {
  const internal = sender.url?.startsWith(chrome.runtime.getURL(''));
  if (m.type === 'hello') return {session: await contextFor(sender, await read())};
  if (m.type === 'candidate') {
    const run = await read();
    if (!run || m.id !== run.id || run.phase !== 'ARMED' || !await contextFor(sender, run)) return {ok: false};
    if (Date.now() < run.notBefore) return {ok: false};
    if (!await activeTab(run.tabId)) { await stop('예매 탭이 전면이 아니어서 중지했습니다.'); return {ok: false}; }
    // A fresh DOM check is mandatory immediately before a trusted pointer event.
    const point = await chrome.tabs.sendMessage(run.tabId, {type: 'measure', config: run.config}, {frameId: 0});
    if (!point?.ok) { await stop(point?.error || '버튼을 다시 확인하지 못했습니다.'); return {ok: false}; }
    run.phase = nextPhase(run.phase, 'CLICK'); await save(run); // persist claim before external effect; never retry
    try {
      await chrome.debugger.sendCommand({tabId: run.tabId}, 'Input.dispatchMouseEvent', {type: 'mousePressed', button: 'left', clickCount: 1, x: point.x, y: point.y});
      await chrome.debugger.sendCommand({tabId: run.tabId}, 'Input.dispatchMouseEvent', {type: 'mouseReleased', button: 'left', clickCount: 1, x: point.x, y: point.y});
    } catch (e) { await stop('클릭 결과가 불확실해 재시도 없이 중지: ' + e.message); return {ok: false}; }
    await record('예매 버튼 1회 클릭. 로그인·대기열·회차 선택은 사이트 화면을 확인하세요.');
    if (!run.config.autoMacro) { await stop('예매 진입 클릭 완료. 자동 좌석 시작은 설정되지 않았습니다.', 'DONE'); return {ok: true}; }
    await chrome.action.setBadgeText({text: '준비'});
    // New documents call hello. An already-mounted same-page frame also needs a new phase.
    await tell(run.tabId, {type: 'resync'});
    return {ok: true};
  }
  if (m.type === 'ready') {
    const run = await read();
    if (!run || m.id !== run.id || !await contextFor(sender, run)) return {ok: false};
    if (!await activeTab(sender.tab.id)) { await stop('좌석 탭이 전면이 아니어서 매크로 시작을 중지했습니다.'); return {ok: false}; }
    const ready = await chrome.tabs.sendMessage(sender.tab.id, {type: 'readyCheck', config: run.config}, {frameId: sender.frameId});
    if (!ready?.ok) { await stop('좌석 준비 조건 또는 포커스가 바뀌어 중지했습니다.'); return {ok: false}; }
    run.phase = nextPhase(run.phase, 'START'); await save(run);
    try {
      if(run.config.seatDriver==='browser') {
        await stop('좌석 준비 확인. 브라우저 좌석 처리를 시작합니다.','DONE');
        return await seats.start(sender.tab.id,(await chrome.storage.local.get('profile')).profile);
      }
      const result = await nativeExchange({op: 'start', browser: run.config.browser, requestId: run.id});
      if (!result?.ok) throw new Error(result?.detail || result?.error || '매크로가 시작 요청을 거절했습니다.');
      await stop('Windows 매크로 시작 요청이 수락되었습니다. 이후 긴급 중지는 F3입니다.', 'DONE');
      return {ok: true};
    } catch (e) { await stop('매크로 연결 실패: ' + e.message); return {ok: false}; }
  }
  if (m.type === 'expired' || m.type === 'contentError') {
    const run = await read();
    if (run && m.id === run.id && await contextFor(sender, {...run, expiresAt: Infinity})) await stop(m.type === 'expired' ? '최대 대기 시간을 넘겨 중지했습니다.' : '페이지 감지 실패: ' + String(m.error).slice(0,300));
    return {ok: true};
  }
  if (m.type === 'picked') {
    if (!sender.tab || !allowedUrl(sender.url)) throw new Error('허용하지 않은 페이지입니다.');
    const {picking} = await chrome.storage.session.get('picking');
    if (!picking || picking.tabId !== sender.tab.id || picking.kind !== m.kind || Date.now() > picking.expiresAt) throw new Error('요소 선택 시간이 만료되었습니다.');
    if (m.kind === 'button' && sender.frameId !== 0) throw new Error('예매 진입 버튼은 최상위 페이지에서 지정하세요.');
    const {config = {}} = await chrome.storage.local.get('config');
    if (m.kind === 'button') Object.assign(config, {buttonSelector: m.info.selector, buttonText: m.info.text, url: m.info.url});
    else Object.assign(config, {readySelector: m.info.selector, readyText: '', readyUrl: m.info.url});
    await chrome.storage.local.set({config}); await chrome.storage.session.remove('picking');
    await tell(sender.tab.id, {type: 'clear'}); await record('요소 저장 완료. 확장 아이콘을 다시 열어 확인하세요.');
    return {ok: true};
  }
  if(m.type==='seatGo')return seats.go(m,sender);
  if(m.type==='seatGateBlocked')return seats.gateBlocked(m,sender);
  if(m.type==='seatStageReady')return seats.stageReady(m,sender);
  if(m.type==='seatDocumentReady')return seats.documentReady(sender);
  if(m.type==='seatOutcome')return seats.outcome(m,sender);
  if(m.type==='seatBindingPicked') {
    const {seatPicking}=await chrome.storage.session.get('seatPicking');
    if(!seatPicking||seatPicking.tabId!==sender.tab?.id||!seatUrl(sender.url)||m.kind!==seatPicking.kind||Date.now()>seatPicking.expiresAt||!m.binding?.selector)throw new Error('지도 지정 시간이 만료되었습니다.');
    await chrome.storage.session.remove('seatPicking');
    const {profile={mode:'pixels',after:[]}}=await chrome.storage.local.get('profile');
    const binding={...m.binding,frameUrl:sender.frameId===0?'':sender.url.split('?')[0].split('#')[0]};
    if(m.kind==='region'){
      profile.entrance??={enabled:true};profile.entrance.region=binding;
      if(!profile.entrance.readySelector){profile.entrance.readySelector=binding.anchorSelector||binding.selector;if(!profile.entrance.frameUrl)profile.entrance.frameUrl=binding.frameUrl;}
    }else if(m.kind==='map'){profile.mapAnchor=binding;profile.mode='pixels';}
    else throw new Error('지원하지 않는 지도 지정입니다.');
    await chrome.storage.local.set({profile});await tell(sender.tab.id,{type:'seatClear'});await record('지도 기준 저장: '+m.kind+' / '+binding.kind);mac.push();return {ok:true,profile};
  }
  if(m.type==='seatPicked') {
    const {seatPicking}=await chrome.storage.session.get('seatPicking');
    if(!seatPicking||seatPicking.tabId!==sender.tab?.id||sender.frameId!==0||!seatUrl(sender.url)||m.kind!==seatPicking.kind||Date.now()>seatPicking.expiresAt)throw new Error('좌표 지정 시간이 만료되었습니다.');
    await chrome.storage.session.remove('seatPicking');
    const {profile={mode:'pixels',after:[]}}=await chrome.storage.local.get('profile');
    const point=m.points?.[0];
    if(!point||!Number.isFinite(point.x)||!Number.isFinite(point.y))throw new Error('유효하지 않은 좌표입니다.');
    if(m.kind==='roi') {
      const last=m.points[1];if(!last||!Number.isFinite(last.x)||!Number.isFinite(last.y))throw new Error('검색 범위 두 점이 필요합니다.');
      profile.mode='pixels';profile.roi={x:Math.min(point.x,last.x),y:Math.min(point.y,last.y),width:Math.abs(last.x-point.x),height:Math.abs(last.y-point.y)};
    } else if(m.kind==='commit')profile.commit=point;
    else if(m.kind==='region'){profile.entrance??={enabled:true};profile.entrance.region=point;}
    else if(m.kind==='after'){profile.after??=[];if(profile.after.length>=20)throw new Error('후속 클릭은 최대 20개입니다.');profile.after.push({...point,delayMs:0});}
    else if(m.kind==='color') {
      const tabId=sender.tab.id;
      await chrome.debugger.attach({tabId},'1.3');
      try {
        const shot=await chrome.debugger.sendCommand({tabId},'Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
        const bitmap=await createImageBitmap(await(await fetch('data:image/png;base64,'+shot.data)).blob());
        const canvas=new OffscreenCanvas(bitmap.width,bitmap.height),ctx=canvas.getContext('2d',{willReadFrequently:true});ctx.drawImage(bitmap,0,0);bitmap.close();
        const x=Math.floor(point.x*canvas.width/m.geometry.width),y=Math.floor(point.y*canvas.height/m.geometry.height);
        const data=ctx.getImageData(x,y,1,1).data;profile.color='#'+[...data].slice(0,3).map(c=>c.toString(16).padStart(2,'0')).join('');
      } finally {await chrome.debugger.detach({tabId}).catch(()=>{});}
    } else throw new Error('알 수 없는 좌표 종류입니다.');
    await chrome.storage.local.set({profile});await record('좌표/색상 저장: '+m.kind);mac.push();return {ok:true,profile};
  }
  if (!internal) throw new Error('설정 화면에서 요청하세요.');
  if (m.type === 'status') return getStatus();
  if(m.type==='macConnect')return mac.connect();
  if(m.type==='seatSaveProfile'){await chrome.storage.local.set({profile:m.profile});mac.push();return {ok:true};}
  if(m.type==='seatStart'){
    await stop('수동 좌석 실행으로 전환합니다.');
    const profile=m.profile||(await chrome.storage.local.get('profile')).profile;
    validateSeatProfile(profile);await chrome.storage.local.set({profile});return seats.start(m.tabId,profile);
  }
  if(m.type==='seatPick'){
    if(seats.busy())throw new Error('좌석 작업을 먼저 중지하세요.');
    const run=await read();if(run&&['ARMED','WAIT_READY','STARTING'].includes(run.phase))throw new Error('예매 대기를 먼저 중지하세요.');
    const tab=await chrome.tabs.get(m.tabId);if(!seatUrl(tab.url))throw new Error('NOL 또는 제공된 연습 화면에서 지정하세요.');
    if(!['roi','commit','after','color','region','map'].includes(m.kind))throw new Error('지원하지 않는 좌표 종류입니다.');
    if(m.profile)await chrome.storage.local.set({profile:m.profile});
    await chrome.storage.session.set({seatPicking:{tabId:m.tabId,kind:m.kind,expiresAt:Date.now()+60000}});
    return ['region','map'].includes(m.kind)?chrome.tabs.sendMessage(m.tabId,{type:'seatPickBinding',kind:m.kind}):chrome.tabs.sendMessage(m.tabId,{type:'seatPick',kind:m.kind},{frameId:0});
  }
  if (m.type === 'save') { await chrome.storage.local.set({config: m.config}); return {ok: true}; }
  if (m.type === 'arm') {if(m.profile)await chrome.storage.local.set({profile:m.profile});await chrome.storage.local.set({config:m.config});return arm(m.config,m.tabId);}
  if (m.type === 'stop') { await seats.stop(); await stop('사용자가 브라우저 대기를 중지했습니다. 실행 중인 Windows 매크로는 F3으로 중지하세요.'); return {ok: true}; }
  if (m.type === 'ping') return chrome.runtime.sendNativeMessage(HOST, {op: 'ping'});
  if (m.type === 'pick') {
    const run = await read();
    if (run && ['ARMED','WAIT_READY','STARTING'].includes(run.phase)) throw new Error('대기를 중지한 뒤 요소를 지정하세요.');
    await chrome.storage.session.set({picking: {tabId: m.tabId, kind: m.kind, expiresAt: Date.now() + 60000}});
    const reply = await chrome.tabs.sendMessage(m.tabId, {type: 'pick', kind: m.kind});
    return reply;
  }
  if (m.type === 'clock') {
    if (!allowedUrl(m.url)) throw new Error('허용된 NOL 페이지에서만 측정할 수 있습니다.');
    const samples = [];
    for (let i = 0; i < 3; i++) {
      const start = Date.now();
      const response = await fetch(m.url, {method: 'HEAD', cache: 'no-store', credentials: 'omit', redirect: 'error', signal: AbortSignal.timeout(5000)});
      const end = Date.now();
      if (!response.ok) throw new Error('서버가 시각 측정 요청에 정상 응답하지 않았습니다.');
      samples.push(clockSample(response.headers.get('Date'), response.headers.get('Age'), start, end));
    }
    const clock = {...samples.sort((a,b) => a.rttMs - b.rttMs)[0], url: m.url};
    await chrome.storage.local.set({clock}); return {ok: true, clock};
  }
  throw new Error('알 수 없는 요청입니다.');
}
chrome.runtime.onMessage.addListener((m, sender, reply) => {
  // hello must not queue behind an in-flight click; it can only read persisted state.
  const promise = ['hello','seatGo','seatOutcome','seatStageReady','seatDocumentReady','seatGateBlocked','stop'].includes(m.type) ? handle(m, sender) : locked(() => handle(m, sender));
  promise.then(reply, async e => { if (['candidate','ready'].includes(m.type)) await stop('자동 진행 오류: ' + e.message); reply({ok: false, error: e.message}); });
  return true;
});
chrome.alarms.onAlarm.addListener(alarm => { if (alarm.name === 'expiry') locked(async () => { const run = await read(); if (run && ['ARMED','WAIT_READY','STARTING'].includes(run.phase)) await stop('최대 대기 시간 만료'); }); });
chrome.debugger.onDetach.addListener(source => { locked(async () => { const run = await read(); if (run?.tabId === source.tabId && ['ARMED','WAIT_READY'].includes(run.phase)) await stop('브라우저 디버거 연결이 종료되어 대기를 중지했습니다.'); }); });
chrome.tabs.onRemoved.addListener(tabId => { locked(async () => { const run = await read(); if (run?.tabId === tabId && ['ARMED','WAIT_READY'].includes(run.phase)) await stop('상품 탭이 닫혀 중지했습니다.'); }); });
chrome.tabs.onUpdated.addListener((tabId, change) => { if (change.url) locked(async () => { const run = await read(); if (run?.tabId === tabId && run.phase === 'ARMED' && !samePage(change.url, run.config.url)) await stop('대기 중 상품 페이지가 바뀌어 중지했습니다.'); }); });

chrome.debugger.onEvent?.addListener((source,method,params)=>{if(method==='Page.javascriptDialogOpening')seats.dialog(source,params);});
chrome.debugger.onDetach.addListener(source=>{if(seats.busy()&&seats.run.tabId===source.tabId)seats.stop('브라우저 연결이 종료되었습니다.');});
chrome.tabs.onRemoved.addListener(tabId=>{if(seats.busy()&&seats.run.tabId===tabId)seats.stop('좌석 탭이 닫혔습니다.');});
chrome.tabs.onUpdated.addListener((tabId,change)=>{if(change.url&&seats.busy()&&seats.run.tabId===tabId)seats.stop('좌석 페이지가 이동하여 중지했습니다. 결과를 직접 확인하세요.');});
if(typeof ServiceWorkerGlobalScope!=='undefined'&&/Mac/.test(globalThis.navigator?.platform||''))mac.connect();

chrome.commands?.onCommand.addListener(command=>{if(command==='stop')handle({type:'stop'},{url:chrome.runtime.getURL('keyboard-stop')}).catch(()=>{});});

// Real MV3 service worker/content script/debugger integration. All HTTP requests are mocked.
// Native messaging is mocked because Windows is not available on the development Mac.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.PLAYWRIGHT_MODULE || 'playwright-core');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const extension=path.join(root,'extension');
const context=await chromium.launchPersistentContext('',{headless:true,channel:'chromium',args:[`--disable-extensions-except=${extension}`,`--load-extension=${extension}`]});
const source='https://nol.yanolja.com/ticket/products/26010352';
let requests=0;
await context.route('**/*',route=>{
  if(route.request().url().startsWith('chrome-extension://')) return route.continue();
  if(route.request().url()===source){requests++;return route.fulfill({contentType:'text/html; charset=utf-8',body:`<!doctype html><meta charset="utf-8"><title>LOCAL FIXTURE ONLY</title><h1>모의 예매</h1><aside id="booking-aside-container"><button id="book" disabled>예매하기</button></aside><div id="seat" hidden>모의 좌석 준비</div><script>window.clicks=0;window.trusted=false;document.querySelector('#book').onclick=e=>{clicks++;trusted=e.isTrusted;document.querySelector('#seat').hidden=false;};</script>`});}
  return route.abort();
});
try{
  const sw=context.serviceWorkers()[0] || await context.waitForEvent('serviceworker');
  const id=new URL(sw.url()).host;
  const product=await context.newPage();await product.goto(source);
  const ui=await context.newPage();ui.on('pageerror',e=>console.error('UI error:',e.message));await ui.goto(`chrome-extension://${id}/popup.html`);
  try {await ui.waitForFunction(()=>document.querySelector('#extensionId').textContent.includes('확장 ID'),null,{timeout:5000});}
  catch(e){console.error('Popup status:',await ui.locator('#message').textContent());console.error('Worker:',sw.url());throw e;}
  const tabId=await sw.evaluate(async url=>(await chrome.tabs.query({url}))[0].id,source);
  const config={url:source,buttonSelector:'#booking-aside-container button',buttonText:'예매하기',saleAt:'',timeoutMinutes:2,autoMacro:false,readyUrl:'',readySelector:'',readyText:'',browser:'chrome'};
  await fs.mkdir(path.join(root,'verification'),{recursive:true});
  await ui.screenshot({path:path.join(root,'verification/popup.png'),fullPage:true});
  const arm=await ui.evaluate(async args=>chrome.runtime.sendMessage({type:'arm',...args}),{config,tabId});assert.equal(arm.ok,true,JSON.stringify(arm));
  await product.bringToFront();
  await product.locator('#book').evaluate(e=>e.disabled=false);
  try {await product.waitForFunction(()=>clicks===1,null,{timeout:5000});}
  catch(e){console.error('Fixture:',await product.evaluate(()=>({focus:document.hasFocus(),visibility:document.visibilityState,clicks,trusted})));console.error('Extension:',await sw.evaluate(async()=>({session:await chrome.storage.session.get('run'),logs:await chrome.storage.local.get('logs'),windows:await chrome.windows.getAll()})));throw e;}
  assert.equal(await product.evaluate(()=>trusted),true,'debugger must produce trusted pointer input');
  await product.locator('#book').evaluate(e=>{e.dataset.change='again';});
  await product.waitForTimeout(300);
  assert.equal(await product.evaluate(()=>clicks),1);
  const first=await sw.evaluate(async()=>(await chrome.storage.session.get('run')).run);
  assert.equal(first.phase,'DONE');
  console.log('PASS real extension: disabled→enabled, trusted input, single click, completion');

  // Native port mock is isolated to this test worker; no real host is registered or called.
  await sw.evaluate(()=>{
    globalThis.nativeCalls=[];
    chrome.runtime.connectNative=()=>{
      let receive;
      return {onMessage:{addListener(f){receive=f;}},onDisconnect:{addListener(){}},disconnect(){},postMessage(m){nativeCalls.push(m);queueMicrotask(()=>receive(m.op==='ping'?{ok:true,macroRunning:true}:{ok:true,accepted:true}));}};
    };
  });
  await product.reload();
  await ui.bringToFront();
  const autoConfig={...config,autoMacro:true,readyUrl:source,readySelector:'#seat'};
  const arm2=await ui.evaluate(async args=>chrome.runtime.sendMessage({type:'arm',...args}),{config:autoConfig,tabId});assert.equal(arm2.ok,true,JSON.stringify(arm2));
  await product.bringToFront();await product.locator('#book').evaluate(e=>e.disabled=false);
  await product.waitForFunction(()=>clicks===1);
  const deadline=Date.now()+10000;let phase;
  do{phase=await sw.evaluate(async()=>(await chrome.storage.session.get('run')).run?.phase);if(phase==='DONE')break;await product.waitForTimeout(50);}while(Date.now()<deadline);
  assert.equal(phase,'DONE',JSON.stringify(await sw.evaluate(async()=>chrome.storage.local.get('logs'))));
  const native=await sw.evaluate(()=>nativeCalls);
  assert.equal(native.filter(m=>m.op==='start').length,1);
  assert.equal(native[0].op,'ping');
  console.log('PASS real extension: same-page ready condition→one warmed native start (mock host)');
  console.log(`2 MV3 integration scenarios passed; ${requests} local page responses; all other HTTP blocked`);
}finally{await context.close();}

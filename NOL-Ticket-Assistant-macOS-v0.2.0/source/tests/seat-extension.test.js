import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),{chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright-core');
const root=process.cwd(),source='https://nol.yanolja.com/ticket/products/26010352';
const body=await fs.readFile('rehearsal-competition.html','utf8');
const context=await chromium.launchPersistentContext('',{headless:true,channel:'chromium',viewport:{width:1280,height:950},args:[`--disable-extensions-except=${root}/extension`,`--load-extension=${root}/extension`]});
const samples={dom:[],alert:[]};let passed=0;
await context.route('**/*',r=>r.request().url().startsWith('chrome-extension://')?r.continue():r.request().url()===source?r.fulfill({contentType:'text/html; charset=utf-8',body}):r.abort());
try{
  const sw=context.serviceWorkers()[0]||await context.waitForEvent('serviceworker');
  const product=await context.newPage();product.on('dialog',()=>{});await product.goto(source);
  const ui=await context.newPage();await ui.goto(`chrome-extension://${new URL(sw.url()).host}/popup.html`);
  const tabId=await sw.evaluate(async url=>(await chrome.tabs.query({url}))[0].id,source);
  const status=()=>sw.evaluate(async()=>(await chrome.storage.session.get('seatRun')).seatRun);
  async function run({variant='dom',failures=3,latency=40,commit=false,override={},geometry=false}={}){
    await product.reload();
    const profile=await product.evaluate(({variant,failures,latency,commit,geometry})=>{
      document.querySelector('#variant').value=variant;document.querySelector('#failures').value=failures;document.querySelector('#latency').value=latency;document.querySelector('#requireCommit').checked=commit;
      if(geometry){const obs=new MutationObserver(()=>{if(!document.querySelector('#conflict').hidden){document.body.style.height='3000px';scrollTo(0,50);obs.disconnect();}});obs.observe(document.querySelector('#conflict'),{attributes:true});}
      return {...practiceProfile,...(commit?{commit:{selector:'#commit'}}:{})};
    },{variant,failures,latency,commit,geometry});
    await ui.bringToFront();
    const result=await ui.evaluate(args=>chrome.runtime.sendMessage({type:'seatStart',...args}),{tabId,profile:{...profile,...override}});
    assert.equal(result.ok,true,JSON.stringify(result));await product.bringToFront();
    let current;const deadline=Date.now()+10000;
    do{current=await status();if(['DONE','STOPPED'].includes(current?.phase))break;await new Promise(r=>setTimeout(r,15));}while(Date.now()<deadline);
    const page=await product.evaluate(()=>results);
    assert.ok(['DONE','STOPPED'].includes(current?.phase),JSON.stringify({current,page}));
    assert.ok(page.trusted.every(Boolean),'real trusted pointer events');
    assert.equal(new Set(page.clicks).size,page.clicks.length,'never click a failed candidate twice');
    return {current,page};
  }
  for(const variant of ['dom','alert']){
    const {current,page}=await run({variant});assert.equal(current.phase,'DONE',JSON.stringify(current));assert.deepEqual(page.clicks,[0,1,2,3]);assert.deepEqual(page.after,['1','2']);samples[variant].push(...page.gaps);passed++;console.log('PASS '+variant+' rejection → next unique candidate → confirmed success → ordered after clicks');
  }
  const commit=await run({commit:true});assert.equal(commit.current.phase,'DONE',JSON.stringify(commit));assert.equal(commit.page.clicks.length,4);passed++;console.log('PASS explicit seat completion button before result');
  const maximum=await run({failures:8,override:{maxAttempts:2}});assert.equal(maximum.current.phase,'STOPPED');assert.equal(maximum.page.clicks.length,2);passed++;console.log('PASS bounded attempts');
  const unknown=await run({variant:'unknown'});assert.equal(unknown.current.phase,'STOPPED');assert.equal(unknown.page.clicks.length,1);assert.match(unknown.current.message,/확인되지 않은/);passed++;console.log('PASS unknown message stops without accepting');
  const timeout=await run({variant:'timeout',override:{timeoutMs:250}});assert.equal(timeout.current.phase,'STOPPED');assert.equal(timeout.page.clicks.length,1);passed++;console.log('PASS missing outcome stops without retry');
  const geometry=await run({geometry:true});assert.equal(geometry.current.phase,'STOPPED');assert.equal(geometry.page.clicks.length,1);passed++;console.log('PASS changed scroll cancels candidate cache');
  // Pixel pool includes no DOM seat identification, only a bounded screenshot of the map.
  await product.reload();const roi=await product.locator('#seats').evaluate(e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};});
  const pixel=await run({override:{mode:'pixels',roi,color:'#8b5cf6',tolerance:3,minArea:4,maxArea:2500}});assert.equal(pixel.current.phase,'DONE',JSON.stringify(pixel));assert.equal(pixel.page.clicks.length,4);passed++;console.log('PASS screenshot pixel components → conflict recovery');
  for(const variant of ['dom','alert'])for(let i=0;i<15;i++){
    const {current,page}=await run({variant,failures:7,latency:20});assert.equal(current.phase,'DONE',JSON.stringify(current));samples[variant].push(...page.gaps);
  }
  const stats=Object.fromEntries(Object.entries(samples).map(([kind,values])=>{const a=values.sort((a,b)=>a-b);return [kind,{count:a.length,p50Ms:a[Math.floor(a.length*.5)],p95Ms:a[Math.floor(a.length*.95)],minMs:a[0],maxMs:a.at(-1),under10ms:a.filter(x=>x<10).length}];}));
  await fs.mkdir('verification',{recursive:true});await fs.writeFile('verification/seat-benchmark.json',JSON.stringify({createdAt:new Date().toISOString(),environment:'macOS / headless Chrome for Testing / fully local NOL URL fixture / one target tab',metric:'same-page performance.now(): rejection exposed to next trusted seat click; includes dismissal; excludes preceding simulated server wait',notMeasured:'NOL server seat acquisition; actual live site compatibility; Windows latency',scenariosPassed:passed,stats},null,2));
  console.log(JSON.stringify({passed,stats},null,2));
}catch(e){console.error(e);process.exitCode=1;}finally{await context.close();}

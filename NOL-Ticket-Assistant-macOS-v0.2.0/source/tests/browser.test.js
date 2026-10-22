// Local controlled fixtures, with extension messaging mocked. No live ticket requests.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.PLAYWRIGHT_MODULE || 'playwright-core');
const options={headless:true};
if(process.env.CHROME_PATH) options.executablePath=process.env.CHROME_PATH;
else if(process.platform==='darwin') options.executablePath='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
else options.channel='chrome';
const browser=await chromium.launch(options);
const page=await browser.newPage();
const dom=await fs.readFile(new URL('../extension/dom.js',import.meta.url),'utf8');
const content=await fs.readFile(new URL('../extension/content.js',import.meta.url),'utf8');
let count=0;
async function scenario(name,fn){await fn();count++;console.log('PASS '+name);}
try {
  await page.setContent('<button id="top">예매하기</button><aside id="booking-aside-container"><button id="book" disabled>예매하기</button></aside>');
  await page.addScriptTag({content:dom});
  await scenario('disabled, exact scope, duplicate text, covering overlay',async()=>{
    assert.equal(await page.evaluate(()=>NolDom.unique({buttonText:'예매하기'})),null);
    assert.equal(await page.evaluate(()=>NolDom.measure(document.querySelector('#book'))),null);
    await page.locator('#book').evaluate(e=>e.disabled=false);
    assert.ok(await page.evaluate(()=>NolDom.measure(NolDom.unique({buttonSelector:'#booking-aside-container button',buttonText:'예매하기'}))));
    await page.evaluate(()=>{const d=document.createElement('div');d.id='cover';d.style.cssText='position:fixed;inset:0;background:white;z-index:9999';document.body.append(d);});
    assert.equal(await page.evaluate(()=>NolDom.measure(document.querySelector('#book'))),null);
    await page.locator('#cover').evaluate(e=>e.remove());
  });
  await scenario('MutationObserver triggers once when disabled button is replaced/enabled',async()=>{
    await page.locator('#book').evaluate(e=>e.disabled=true);
    await page.evaluate(()=>{
      window.messages=[];window.chrome={runtime:{onMessage:{addListener(fn){window.receive=fn;}},async sendMessage(m){messages.push(m);return {ok:true};}}};
    });
    await page.addScriptTag({content});
    await page.evaluate(()=>receive({type:'monitor',session:{id:'run1',phase:'ARMED',notBefore:0,expiresAt:Date.now()+10000,config:{buttonSelector:'#book',buttonText:'예매하기'}}},{},()=>{}));
    await page.locator('#book').evaluate(e=>e.outerHTML='<button id="book">예매하기</button>');
    await page.waitForFunction(()=>messages.filter(m=>m.type==='candidate').length===1);
    await page.locator('#book').evaluate(e=>{e.dataset.test='1';e.className='other';});
    await page.waitForTimeout(350);
    assert.equal(await page.evaluate(()=>messages.filter(m=>m.type==='candidate').length),1);
  });
  await scenario('stop cancels stale observer; scheduled trigger does not click early',async()=>{
    await page.evaluate(()=>{receive({type:'clear'},{},()=>{});window.messages=[];receive({type:'monitor',session:{id:'run2',phase:'ARMED',notBefore:Date.now()+450,expiresAt:Date.now()+10000,config:{buttonSelector:'#book',buttonText:'예매하기'}}},{},()=>{});});
    await page.waitForTimeout(150);assert.equal(await page.evaluate(()=>messages.filter(m=>m.type==='candidate').length),0);
    await page.waitForFunction(()=>messages.some(m=>m.type==='candidate'));
    await page.evaluate(()=>{receive({type:'clear'},{},()=>{});window.messages=[];document.querySelector('#book').className='again';});
    await page.waitForTimeout(350);assert.equal(await page.evaluate(()=>messages.filter(m=>m.type==='candidate').length),0);
  });
  await scenario('picker captures selector without triggering actual button',async()=>{
    await page.evaluate(()=>{window.clicked=0;document.querySelector('#book').onclick=()=>clicked++;receive({type:'pick',kind:'button'},{},()=>{});});
    await page.locator('#book').click();
    assert.equal(await page.evaluate(()=>clicked),0);
    assert.equal(await page.evaluate(()=>messages.find(m=>m.type==='picked').info.selector),'#book');
  });
  console.log(`${count} browser fixture scenarios passed`);
}finally{await browser.close();}

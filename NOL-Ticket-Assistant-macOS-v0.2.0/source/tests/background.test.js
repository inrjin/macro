import test from 'node:test';
import assert from 'node:assert/strict';
const url='https://nol.yanolja.com/ticket/products/26010352';
const seat='https://ticket.interpark.com/Book/Seat';
const clone=x=>structuredClone(x);
let instance=0;
async function harness(extra={}) {
  const db={session:{},local:{}}, calls={click:[],native:[],detaches:0};
  let listener;
  const event=()=>({addListener(){}});
  const state={active:true,focused:true,measure:true,ready:true,nativeOK:true,...extra};
  const tabs=new Map([[5,{id:5,url,active:true,windowId:9}],[6,{id:6,url:seat,openerTabId:5,active:true,windowId:9}],[7,{id:7,url:seat,active:true,windowId:9}]]);
  const storage=area=>({
    async get(keys){if(typeof keys==='string')keys=[keys];return clone(Object.fromEntries(keys.filter(k=>k in db[area]).map(k=>[k,db[area][k]])));},
    async set(value){Object.assign(db[area],clone(value));},async remove(key){delete db[area][key];}
  });
  globalThis.chrome={
    storage:{session:storage('session'),local:storage('local')},
    runtime:{id:'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',getURL:p=>'chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/'+p,
      connectNative(){let receive;return {onMessage:{addListener(fn){receive=fn;}},onDisconnect:event(),disconnect(){},postMessage(m){calls.native.push(m);queueMicrotask(()=>receive(m.op==='ping'?{ok:true,macroRunning:true}:{ok:state.nativeOK,error:'macro_refused'}));}};},
      onMessage:{addListener(fn){listener=fn;}},async sendNativeMessage(host,m){calls.native.push(m);return m.op==='ping'?{ok:true,macroRunning:true}:{ok:state.nativeOK,error:'macro_refused'};}},
    tabs:{async get(id){return {...tabs.get(id),active:state.active};},async query(){return [...tabs.values()];},
      async sendMessage(id,m){if(m.type==='measure')return {ok:state.measure,x:200,y:100};if(m.type==='readyCheck')return {ok:state.ready};return {ok:true};},onRemoved:event(),onUpdated:event()},
    windows:{async get(){return {focused:state.focused};}},
    debugger:{async attach(){},async detach(){calls.detaches++;},async sendCommand(t,method,params){calls.click.push({method,params});},onDetach:event()},
    alarms:{async clear(){},async create(){},onAlarm:event()},action:{async setBadgeText(){}}
  };
  await import('../extension/background.js?test='+instance++);
  const config={url,buttonSelector:'#booking-aside-container button',buttonText:'예매하기',timeoutMinutes:30,saleAt:'',autoMacro:true,readyUrl:seat,readySelector:'#seat',browser:'chrome'};
  const source={tab:tabs.get(5),frameId:0,url};
  const admin={url:'chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/popup.html'};
  const send=(m,s=admin)=>new Promise(resolve=>listener(m,s,resolve));
  const arm=()=>send({type:'arm',config,tabId:5});
  return {db,calls,state,config,source,admin,send,arm,tabs};
}
test('two simultaneous DOM events click once; matching ready frame starts once',async()=>{
  const h=await harness();assert.ok((await h.arm()).ok);
  const id=h.db.session.run.id;
  await Promise.all([h.send({type:'candidate',id},h.source),h.send({type:'candidate',id},h.source)]);
  assert.equal(h.calls.click.length,2);assert.equal(h.db.session.run.phase,'WAIT_READY');
  const frame={tab:h.tabs.get(6),frameId:3,url:seat};
  await Promise.all([h.send({type:'ready',id},frame),h.send({type:'ready',id},frame)]);
  assert.equal(h.calls.native.filter(m=>m.op==='start').length,1);
  assert.equal(h.db.session.run.phase,'DONE');
});
test('unrelated same-URL tab and wrong iframe URL cannot start native macro',async()=>{
  const h=await harness();await h.arm();const id=h.db.session.run.id;
  await h.send({type:'candidate',id},h.source);
  await h.send({type:'ready',id},{tab:h.tabs.get(7),frameId:0,url:seat});
  await h.send({type:'ready',id},{tab:h.tabs.get(6),frameId:1,url:seat+'Fake'});
  assert.equal(h.calls.native.filter(m=>m.op==='start').length,0);
});
test('fresh covered-button check prevents click and stops',async()=>{
  const h=await harness({measure:false});await h.arm();await h.send({type:'candidate',id:h.db.session.run.id},h.source);
  assert.equal(h.calls.click.length,0);assert.equal(h.db.session.run.phase,'STOPPED');
});
test('loss of focused window prevents click',async()=>{
  const h=await harness({focused:false});await h.arm();await h.send({type:'candidate',id:h.db.session.run.id},h.source);
  assert.equal(h.calls.click.length,0);assert.equal(h.db.session.run.phase,'STOPPED');
});
test('native refusal never automatically retries',async()=>{
  const h=await harness({nativeOK:false});await h.arm();const id=h.db.session.run.id;
  await h.send({type:'candidate',id},h.source);const frame={tab:h.tabs.get(6),frameId:0,url:seat};
  await h.send({type:'ready',id},frame);await h.send({type:'ready',id},frame);
  assert.equal(h.calls.native.filter(m=>m.op==='start').length,1);assert.equal(h.db.session.run.phase,'STOPPED');
});
test('page scripts cannot change configuration or arm',async()=>{
  const h=await harness();const r=await h.send({type:'arm',config:h.config,tabId:5},h.source);
  assert.equal(r.ok,false);assert.equal(h.db.session.run,undefined);
});
test('scheduled time cannot be advanced by HTTP Date sample',async()=>{
  const h=await harness();const epoch=Date.now()+300000;
  h.config.saleAt=new Date(epoch+9*3600000).toISOString().slice(0,19);
  h.db.local.clock={url,offsetMs:100000,sampledAt:Date.now()};
  await h.arm();assert.ok(Math.abs(h.db.session.run.notBefore-epoch)<1000);
  await h.send({type:'candidate',id:h.db.session.run.id},h.source);assert.equal(h.calls.click.length,0);
});

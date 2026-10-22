import test from 'node:test';import assert from 'node:assert/strict';
import {macLink} from '../extension/mac-link.js';
test('Mac commands route to selected browser, deduplicate, and reject unknown actions',async()=>{
 let receive,disconnect;const messages=[],handled=[],logs=[];
 globalThis.chrome={runtime:{connectNative(name){assert.equal(name,'com.nol_ticket_assistant.mac');return {onMessage:{addListener(f){receive=f;}},onDisconnect:{addListener(f){disconnect=f;}},postMessage(m){messages.push(m);}};}},tabs:{async query(){return [{id:7}];}}};
 const link=macLink(async()=>({profile:{mode:'dom'}}),async m=>handled.push(m),async s=>logs.push(s));
 try{
  link.connect();await link.push();assert.equal(messages[0].op,'ping');assert.equal(messages.at(-1).state.browser,'chrome');
  await receive({op:'appCommand',id:'1',action:'seatStart',payload:{browser:'msedge'}});assert.equal(handled.length,0);
  const m={op:'appCommand',id:'2',action:'seatStart',payload:{browser:'chrome',profile:{mode:'dom'}}};await receive(m);await receive(m);assert.equal(handled.length,1);assert.equal(handled[0].tabId,7);
  await receive({op:'appCommand',id:'3',action:'exec',payload:{browser:'chrome'}});assert.equal(handled.length,1);assert.ok(logs.some(s=>s.includes('지원하지 않는')));
 }finally{disconnect();}
});

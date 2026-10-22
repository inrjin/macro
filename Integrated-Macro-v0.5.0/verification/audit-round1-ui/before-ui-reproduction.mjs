import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { defaultProfile, applyCapture } from '/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/ui/model.js';
import { loadAppPage } from '/Users/inrjin/Documents/機械人間/Integrated-Macro-v0.5.0/ui/tests/dom-fixture.mjs';

const html = new URL('file:///Users/inrjin/Documents/%E6%A9%9F%E6%A2%B0%E4%BA%BA%E9%96%93/Integrated-Macro-v0.5.0/ui/index.html');
const clone = value => JSON.parse(JSON.stringify(value));
const initial = {phase:'idle',runId:0,revision:0,mode:'normal',running:false,paused:false,captureState:'idle',captureField:'',captureArmed:false,captureError:null,message:'',checks:[],manualActions:[],selectedCount:0,targetCount:2};
const frame = {width:1200,height:800,scale_x:1,scale_y:1};
const color = {r:130,g:70,b:210};
const before = applyCapture(defaultProfile(), {field:'vision.roi.start',point:{x:100,y:120},color,frame});
const capture = {field:'vision.roi.end',point:{x:700,y:620},color,frame};
let armPayload, savedPayload, releaseSave;
const page = await loadAppPage(html, initial, async (name,args,emit) => {
  if(name==='arm_capture') { armPayload=clone(args.profile); return {field:args.field,profile:args.profile,snapshot:{...initial,revision:1,captureField:args.field,captureState:'armed',captureArmed:true}}; }
  if(name==='save_profile') {
    savedPayload=clone(args.profile); // Native IPC serializes at dispatch time.
    return new Promise(resolve=>{releaseSave=()=>resolve(clone(savedPayload));}); // Actual runtime.save returns raw profile.
  }
  throw new Error(`Unexpected command: ${name}`);
}, before);
const $=selector=>page.document.querySelector(selector);
await $('[data-capture-field="vision.roi.end"]').click();
page.emit('state-change',{...initial,revision:2,captureField:capture.field,captureState:'reading',captureArmed:true,message:'F8 수신 · 좌석 범위 끝 읽는 중'});
assert.equal($('#save-button').disabled,false,'Actual UI allows a full Save while F8 worker is reading');
const saving=$('#save-button').click();
assert.equal(savedPayload.vision.roi,null);
assert.equal(savedPayload.desktop.capture_points.seat_end,undefined);
// F8's inline native capture/save finishes first. UI receives its point event
// before the queued stale full Save finishes; both orders are supported by IPC.
page.emit('point-captured',capture);
page.emit('state-change',{...initial,revision:3,captureField:capture.field,captureState:'captured',captureArmed:false,message:'좌석 범위 끝 지정 완료'});
releaseSave(); await saving;
assert.match($('[data-point-field="vision.roi.end"]').textContent,/700.*620/);
assert.match($('#save-status').textContent,/설정 저장됨/);
await writeFile('/tmp/macro-ui-stale-save-ingress.json',JSON.stringify({before:armPayload,savePayload:savedPayload,capture},null,2));
console.log('REPRODUCED UI: F8 reading leaves Save enabled; actual Save IPC sends old ROI=null and no seat_end; later point event leaves UI showing captured endpoint despite raw stale Save response.');

let releaseStartSave; const order=[];
const startPage=await loadAppPage(html,initial,async(name,args)=>{
  order.push(name);
  if(name==='save_profile') return new Promise(resolve=>{releaseStartSave=()=>resolve(clone(args.profile));});
  if(name==='start_run') return {...initial,phase:'seats',runId:2,revision:5,running:true,message:'빈 좌석 계속 탐색 중'};
  throw new Error(`Unexpected command: ${name}`);
},defaultProfile());
const starting=startPage.document.querySelector('#start-button').click();
assert.deepEqual(order,['save_profile']);
// A genuine native F4 is independent of UI commandBusy and may complete while
// the preceding Save result is still in transit to this WebView.
startPage.emit('state-change',{...initial,phase:'stopped',runId:1,revision:4,message:'사용자 중지'});
releaseStartSave(); await starting;
assert.deepEqual(order,['save_profile','start_run']);
assert.equal(startPage.document.querySelector('#status-symbol').classList.contains('running'),true);
console.log('REPRODUCED UI: Start click -> pending Save -> native stopped snapshot -> Save resolves -> UI still sends start_run and displays running.');
console.log('Limit: actual unchanged app.js/model.js + VM DOM + controllable IPC transport; no native worker, live WebView, OS input, or user settings touched.');

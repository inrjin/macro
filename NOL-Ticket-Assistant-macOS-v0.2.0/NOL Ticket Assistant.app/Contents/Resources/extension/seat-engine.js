import {validateSeatProfile, pixelCandidates, normalize} from './seat-core.js';
import {allowedUrl, samePage} from './core.js';
export const seatUrl = value => {
  if (allowedUrl(value)) return true;
  try {const u=new URL(value);return u.protocol==='file:' && u.pathname.endsWith('/rehearsal-competition.html') && !u.host;}catch{return false;}
};
export class SeatEngine {
  constructor(changed=()=>{}) {this.changed=changed;this.run=null;this.input=Promise.resolve();}
  status() {const r=this.run;return r?{id:r.id,tabId:r.tabId,phase:r.phase,attempt:r.attempt,count:r.count,message:r.message,startedAt:r.startedAt}:null;}
  busy() {return !!this.run && !['DONE','STOPPED'].includes(this.run.phase);}
  publish() {Promise.resolve(this.changed(this.status())).catch(()=>{});}
  async message(type,extra={},frameId=0) {
    const r=await chrome.tabs.sendMessage(this.run.tabId,{type,...extra},{frameId});
    if(!r?.ok)throw new Error(r?.error||'좌석 화면에 연결되지 않았습니다. 새로고침하세요.');return r;
  }
  async command(method,params={}) {return chrome.debugger.sendCommand({tabId:this.run.tabId},method,params);}
  async start(tabId,input) {
    if(this.busy())throw new Error('이미 좌석 작업이 진행 중입니다.');
    const profile=validateSeatProfile(input),tab=await chrome.tabs.get(tabId);
    if(!seatUrl(tab.url))throw new Error('NOL 좌석 화면 또는 제공된 연습 화면에서 실행하세요.');
    const r=this.run={id:crypto.randomUUID(),tabId,url:tab.url,profile,phase:'WAIT_FOCUS',attempt:0,index:0,count:0,startedAt:Date.now(),attached:false};
    this.publish();
    try {
      await chrome.debugger.attach({tabId},'1.3');r.attached=true;
      await this.command('Page.enable');
      r.frameId=0;
      if(profile.outcomeFrameUrl && !samePage(profile.outcomeFrameUrl,tab.url)) {
        if(!allowedUrl(profile.outcomeFrameUrl))throw new Error('결과 프레임은 허용된 NOL 주소여야 합니다.');
        const frames=(await chrome.webNavigation.getAllFrames({tabId})).filter(f=>samePage(f.url,profile.outcomeFrameUrl));
        if(frames.length!==1)throw new Error('결과를 확인할 프레임을 하나로 특정하지 못했습니다.');
        r.frameId=frames[0].frameId;
      }
      r.timer=setTimeout(()=>this.stop('브라우저 포커스를 기다리다 중지했습니다.'),10000);
      await this.message('seatAwaitFocus',{runId:r.id});
      return {ok:true};
    }catch(e){await this.stop(e.message);throw e;}
  }
  async go(m,sender) {
    const r=this.run;
    if(!r||r.phase!=='WAIT_FOCUS'||r.id!==m.runId||sender.tab?.id!==r.tabId||sender.frameId!==0)return {ok:false};
    try {await this.focused();}catch{return {ok:false};}
    r.phase='PREPARING';clearTimeout(r.timer);this.publish();
    try {
      const prepared=await this.message('seatPrepare',{selector:r.profile.mode==='dom'?r.profile.candidateSelector:null});
      if(r.profile.mode==='pixels') {
        const roi=r.profile.roi,g=prepared.geometry;
        if(roi.x+roi.width>g.width||roi.y+roi.height>g.height)throw new Error('좌석 검색 범위가 현재 화면 밖입니다.');
        // Capture the existing viewport without a CDP clip: clipping can resize emulated viewports.
        const shot=await this.command('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
        const blob=await (await fetch('data:image/png;base64,'+shot.data)).blob();const bitmap=await createImageBitmap(blob);
        const canvas=new OffscreenCanvas(Math.ceil(roi.width),Math.ceil(roi.height)),ctx=canvas.getContext('2d',{willReadFrequently:true});
        const scaleX=bitmap.width/g.width,scaleY=bitmap.height/g.height;
        ctx.drawImage(bitmap,roi.x*scaleX,roi.y*scaleY,roi.width*scaleX,roi.height*scaleY,0,0,canvas.width,canvas.height);bitmap.close();
        r.points=pixelCandidates(ctx.getImageData(0,0,canvas.width,canvas.height).data,canvas.width,canvas.height,r.profile);r.count=r.points.length;
      }else r.count=prepared.count;
      if(!r.count)throw new Error('지정한 범위에 좌석 후보가 없습니다.');
      if(!this.alive(r))return {ok:false};
      await this.next(r);return {ok:true};
    }catch(e){await this.stop(e.message);return {ok:false,error:e.message};}
  }
  alive(r) {return r===this.run && !['DONE','STOPPED'].includes(r.phase);}
  async focused() {
    const r=this.run,tab=await chrome.tabs.get(r.tabId),win=await chrome.windows.get(tab.windowId);
    if(!tab.active||!win.focused||!samePage(tab.url,r.url))throw new Error('실행 탭·주소·창 포커스가 바뀌었습니다.');
  }
  async click(point,r=this.run) {
    if(!this.alive(r))return;
    const task=(async()=>{
      await this.command('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',clickCount:1,x:point.x,y:point.y});
      // Always release an issued press, including when the user stopped in between.
      await this.command('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',clickCount:1,x:point.x,y:point.y});
    })();
    this.input=task;await task;
  }
  async step(step,r=this.run) {
    const phase=r.phase;
    if(step.delayMs)await new Promise(resolve=>setTimeout(resolve,step.delayMs));
    if(!this.alive(r)||r.phase!==phase)return;
    await this.focused();
    const {point}=await this.message('seatPoint',{step});
    if(this.alive(r)&&r.phase===phase)await this.click(point,r);
  }
  async next(r) {
    if(!this.alive(r))return;
    if(r.attempt>=r.profile.maxAttempts)throw new Error('설정한 최대 시도 횟수에 도달했습니다.');
    await this.focused();
    let point;
    while(r.index<r.count&&!point) {
      const index=r.index++;
      point=r.points?(await this.message('seatPoint',{step:r.points[index]})).point:(await this.message('seatCandidate',{index})).point;
    }
    if(!point)throw new Error('준비한 좌석 후보를 모두 확인했습니다.');
    if(!this.alive(r))return;
    r.attempt++;r.phase='WAIT_OUTCOME';
    await this.message('seatWatch',{profile:r.profile,runId:r.id,attempt:r.attempt},r.frameId);
    if(!this.alive(r))return;
    r.timer=setTimeout(()=>this.stop('좌석 결과 응답이 없어 중지했습니다. 선택 여부를 직접 확인하세요.'),r.profile.timeoutMs);
    await this.click(point,r);
    if(this.alive(r)&&r.phase==='WAIT_OUTCOME'&&r.profile.commit)await this.step(r.profile.commit,r);
    this.publish();
  }
  async outcome(m,sender) {
    const r=this.run;
    if(!r||r.id!==m.runId||r.attempt!==m.attempt||r.phase!=='WAIT_OUTCOME'||sender.tab?.id!==r.tabId||sender.frameId!==r.frameId)return {ok:false};
    if(m.kind==='unknown'){await this.stop('확인되지 않은 안내가 나타났습니다. '+(m.error||''));return {ok:true};}
    if(!['success','conflict'].includes(m.kind))return {ok:false};
    r.phase=m.kind==='success'?'AFTER':'RECOVERING';clearTimeout(r.timer);
    try{
      await this.input;if(!this.alive(r))return {ok:false};
      if(m.kind==='success') {
        for(const step of r.profile.after){await this.step(step,r);if(!this.alive(r))return {ok:false};}
        await this.stop('설정한 성공 조건을 확인했습니다. 후속 클릭 완료.', 'DONE');
      }else{
        await this.step(r.profile.dismiss,r);
        await this.message('seatWaitHidden',{selector:r.profile.conflictSelector,timeoutMs:r.profile.timeoutMs},r.frameId);
        await this.next(r);
      }
    }catch(e){await this.stop(e.message);}
    return {ok:true};
  }
  async dialog(source,params) {
    const r=this.run;if(!r||source.tabId!==r.tabId||!this.alive(r))return;
    // Accept ONLY a precisely configured rejection alert; never accept payment/other prompts.
    if(r.phase!=='WAIT_OUTCOME'||params.type!=='alert'||!r.profile.alertText||normalize(params.message)!==r.profile.alertText){await this.stop('알 수 없는 브라우저 알림입니다. 직접 확인하세요.');return;}
    r.phase='RECOVERING';clearTimeout(r.timer);
    try{
      // Do not queue behind mouseReleased: that command may be blocked by this dialog.
      await this.command('Page.handleJavaScriptDialog',{accept:true});
      await this.input;if(!this.alive(r))return;
      await this.message('seatClear',{},r.frameId);
      await this.next(r);
    }catch(e){await this.stop(e.message);}
  }
  async stop(message='사용자가 중지했습니다.',phase='STOPPED') {
    const r=this.run;if(!r||!this.alive(r))return;
    r.phase=phase;r.message=message;clearTimeout(r.timer);
    this.publish();
    await chrome.tabs.sendMessage(r.tabId,{type:'seatClear'}).catch(()=>{});
    // Bound cleanup so a blocking unknown dialog never prevents the stop UI.
    if(r.attached){r.attached=false;await chrome.debugger.detach({tabId:r.tabId}).catch(()=>{});}
  }
}

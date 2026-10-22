(() => {
  const D=globalThis.NolDom;
  let pool=[], geometry, cleanup=()=>{};
  const geo=()=>({width:innerWidth,height:innerHeight,scrollX,scrollY,dpr:devicePixelRatio});
  const matchGeo=()=>geometry && Object.entries(geometry).every(([k,v])=>geo()[k]===v);
  function one(selector,text='') {
    if(!selector)return null;
    let nodes=[...document.querySelectorAll(selector)].filter(D.visible);
    if(text)nodes=nodes.filter(n=>D.normalized(n.innerText||n.textContent)===D.normalized(text));
    if(nodes.length>1)throw new Error('결과/버튼 표식이 둘 이상입니다. 범위를 좁히세요.');
    return nodes[0]||null;
  }
  function watch(m,reply) {
    cleanup(); const p=m.profile;
    if(one(p.successSelector,p.successText)||one(p.conflictSelector)) throw new Error('이전 성공/실패 안내가 아직 남아 있습니다. 확인 후 다시 시작하세요.');
    let sent=false;
    const observer=new MutationObserver(()=>{
      if(sent)return;
      try{
        const success=one(p.successSelector,p.successText), conflict=one(p.conflictSelector);
        const kind=success?'success':conflict?(D.normalized(conflict.innerText||conflict.textContent)===p.conflictText?'conflict':'unknown'):null;
        if(kind){sent=true;cleanup();chrome.runtime.sendMessage({type:'seatOutcome',runId:m.runId,attempt:m.attempt,kind,observedAt:performance.timeOrigin+performance.now()}).catch(()=>{});}
      }catch(e){sent=true;cleanup();chrome.runtime.sendMessage({type:'seatOutcome',runId:m.runId,attempt:m.attempt,kind:'unknown',error:e.message}).catch(()=>{});}
    });
    observer.observe(document.documentElement,{subtree:true,childList:true,attributes:true,characterData:true});
    cleanup=()=>observer.disconnect(); reply({ok:true});
  }
  function waitHidden(m,reply) {
    cleanup();let finished=false,timer;
    const done=ok=>{if(finished)return;finished=true;observer.disconnect();clearTimeout(timer);reply({ok,error:ok?undefined:'실패 안내창이 닫히지 않았습니다.'});};
    const check=()=>{try{if(!one(m.selector))done(true);}catch{done(false);}};
    const observer=new MutationObserver(check);observer.observe(document.documentElement,{subtree:true,childList:true,attributes:true});
    timer=setTimeout(()=>done(false),m.timeoutMs);cleanup=()=>done(false);check();
  }
  chrome.runtime.onMessage.addListener((m,sender,reply)=>{
    if(!m.type?.startsWith('seat'))return;
    try {
      if(m.type==='seatAwaitFocus') {
        if(window!==top)return;
        cleanup();let sent=false;
        const check=()=>{if(!sent&&document.hasFocus()&&document.visibilityState==='visible'){sent=true;chrome.runtime.sendMessage({type:'seatGo',runId:m.runId}).then(r=>{if(!r?.ok)sent=false;},()=>{sent=false;});}};
        const timer=setInterval(check,200);addEventListener('focus',check);document.addEventListener('visibilitychange',check);
        cleanup=()=>{clearInterval(timer);removeEventListener('focus',check);document.removeEventListener('visibilitychange',check);};reply({ok:true});setTimeout(check,0);
      } else if(m.type==='seatPrepare') {
        if(window!==top)throw new Error('후보/좌표는 브라우저 최상위 뷰포트 기준입니다.');
        cleanup();geometry=geo();pool=m.selector?[...document.querySelectorAll(m.selector)].filter(D.enabled).slice(0,1000):[];
        reply({ok:true,count:pool.length,geometry});
      } else if(m.type==='seatCandidate') {
        if(!matchGeo()||!document.hasFocus())throw new Error('탭 포커스·스크롤·확대율·화면 크기가 바뀌었습니다. '+JSON.stringify({before:geometry,now:geo(),focus:document.hasFocus()}));
        const point=D.measure(pool[m.index]);reply({ok:true,point});
      } else if(m.type==='seatPoint') {
        if(!matchGeo()||!document.hasFocus())throw new Error('탭 포커스·스크롤·확대율·화면 크기가 바뀌었습니다. '+JSON.stringify({before:geometry,now:geo(),focus:document.hasFocus()}));
        if(!m.step)throw new Error('클릭 단계를 지정하세요.');
        const point=m.step.selector?D.measure(one(m.step.selector)):m.step;
        if(!point||point.x<0||point.y<0||point.x>=innerWidth||point.y>=innerHeight)throw new Error('클릭 위치가 보이지 않거나 가려져 있습니다.');
        reply({ok:true,point:{x:point.x,y:point.y}});
      } else if(m.type==='seatWatch') {watch(m,reply);}
      else if(m.type==='seatWaitHidden') {waitHidden(m,reply);return true;}
      else if(m.type==='seatClear'||m.type==='seatOutcomeClear') {cleanup();reply({ok:true});}
      else if(m.type==='seatPick') {
        if(window!==top)return;
        cleanup();const overlay=document.createElement('div'),label=document.createElement('div');
        overlay.style.cssText='position:fixed;inset:0;z-index:2147483647;background:rgba(20,70,120,.05);cursor:crosshair';
        label.style.cssText='position:absolute;top:5px;left:5px;background:#163348;color:white;padding:12px;pointer-events:none';
        label.textContent=m.kind==='roi'?'좌석 범위 왼쪽 위, 오른쪽 아래를 차례로 클릭 · Esc 취소':'클릭 좌표를 지정하세요 · Esc 취소';overlay.append(label);document.documentElement.append(overlay);
        const points=[];const close=()=>{overlay.remove();removeEventListener('keydown',key,true);};const key=e=>{if(e.key==='Escape')close();};addEventListener('keydown',key,true);
        overlay.onclick=e=>{e.preventDefault();e.stopPropagation();points.push({x:e.clientX,y:e.clientY});if(m.kind==='roi'&&points.length===1){label.textContent='이제 오른쪽 아래를 클릭하세요';return;}close();requestAnimationFrame(()=>requestAnimationFrame(()=>chrome.runtime.sendMessage({type:'seatPicked',kind:m.kind,points,geometry:geo()}).catch(()=>{})));};
        cleanup=close;reply({ok:true});
      }
    } catch(e) {reply({ok:false,error:e.message});}
  });
})();

// Observe user-completed entry and the selected region's readiness. Never reads or solves CAPTCHA.
(() => {
  const D=globalThis.NolDom;let clear=()=>{},clearBlock=()=>{};
  function unique(selector,text) {
    const items=[...document.querySelectorAll(selector)].filter(D.visible).filter(n=>!text||D.normalized(n.innerText||n.textContent)===D.normalized(text));
    if(items.length>1)throw new Error('진입/지도 준비 요소가 여러 개입니다. 구역을 특정하세요.');
    return items[0]||null;
  }
  function check(c) {
    // A visible human-verification gate always takes precedence over a map behind it.
    if([...document.querySelectorAll(c.blockedSelector)].some(D.visible))return false;
    const ready=unique(c.readySelector,c.readyText);
    if(!ready||!D.enabled(ready))return false;
    const images=[...(ready.matches('img')?[ready]:[]),...ready.querySelectorAll('img')].filter(D.visible);
    return images.every(img=>img.complete&&img.naturalWidth>0);
  }
  chrome.runtime.onMessage.addListener((m,sender,reply)=>{
    try{
      if(m.type==='seatStageWatch') {
        clear();let sent=false;
        const emit=()=>{if(sent)return;try{if(check(m.condition)){sent=true;clear();chrome.runtime.sendMessage({type:'seatStageReady',runId:m.runId,token:m.token}).catch(()=>{});}}catch(e){sent=true;clear();chrome.runtime.sendMessage({type:'seatStageReady',runId:m.runId,token:m.token,error:e.message}).catch(()=>{});}};
        const observer=new MutationObserver(emit);observer.observe(document.documentElement,{subtree:true,attributes:true,childList:true,characterData:true});
        document.addEventListener('load',emit,true);document.addEventListener('transitionend',emit,true);
        // Only a fallback for CSS changes. DOM transitions are handled by the observer immediately.
        const fallback=setInterval(emit,250);
        clear=()=>{observer.disconnect();clearInterval(fallback);document.removeEventListener('load',emit,true);document.removeEventListener('transitionend',emit,true);};
        reply({ok:true});emit();
      }else if(m.type==='seatBlockWatch') {
        clearBlock();let sent=false;
        const inspect=()=>{if(!sent&&[...document.querySelectorAll(m.selector)].some(D.visible)){sent=true;clearBlock();chrome.runtime.sendMessage({type:'seatGateBlocked',runId:m.runId}).catch(()=>{});}};
        const observer=new MutationObserver(inspect);observer.observe(document.documentElement,{subtree:true,attributes:true,childList:true});
        const timer=setInterval(inspect,250);clearBlock=()=>{observer.disconnect();clearInterval(timer);};reply({ok:true});inspect();
      }else if(m.type==='seatStageCheck')reply({ok:true,ready:check(m.condition)});
      else if(m.type==='seatStageClear'||m.type==='seatClear'){clear();if(m.type==='seatClear')clearBlock();reply({ok:true});}
    }catch(e){reply({ok:false,error:e.message});}
  });
  chrome.runtime.sendMessage({type:'seatDocumentReady'}).catch(()=>{});
})();

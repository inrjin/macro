// Live element bindings, measured in the current document's CSS pixels.
(() => {
 const D=globalThis.NolDom;let removePicker=()=>{};const frameRequests=new Map();
 const unique=selector=>{const a=[...document.querySelectorAll(selector)];if(a.length!==1)throw new Error('저장한 지도/구역 요소를 하나로 찾지 못했습니다. 다시 지정하세요.');return a[0];};
 function box(el) {
  if(!D.visible(el))throw new Error('지도/구역 요소가 보이지 않습니다.');
  for(let n=el;n;n=n.parentElement){const m=new DOMMatrixReadOnly(getComputedStyle(n).transform);if(!m.is2D||m.a<=0||m.d<=0||Math.abs(m.b)>1e-6||Math.abs(m.c)>1e-6)throw new Error('회전·기울임 지도는 이 방식으로 보정할 수 없습니다.');}
  const r=el.getBoundingClientRect();let b={x:r.x,y:r.y,width:r.width,height:r.height};
  if(el instanceof HTMLImageElement){
   if(!el.complete||!el.naturalWidth)throw new Error('지도 이미지가 아직 로딩 중입니다.');
   const s=getComputedStyle(el),sx=r.width/el.offsetWidth,sy=r.height/el.offsetHeight;
   const px=v=>parseFloat(v)||0;
   const w=el.clientWidth-px(s.paddingLeft)-px(s.paddingRight),h=el.clientHeight-px(s.paddingTop)-px(s.paddingBottom);
   let iw=w,ih=h;
   if(s.objectFit!=='fill'){
    const factor=s.objectFit==='cover'?Math.max(w/el.naturalWidth,h/el.naturalHeight):s.objectFit==='none'?1:s.objectFit==='scale-down'?Math.min(1,w/el.naturalWidth,h/el.naturalHeight):Math.min(w/el.naturalWidth,h/el.naturalHeight);
    iw=el.naturalWidth*factor;ih=el.naturalHeight*factor;
   }
   const positions=s.objectPosition.split(/\s+/);if(positions.length!==2)throw new Error('복합 이미지 정렬은 지원하지 않습니다.');
   const offset=(v,free)=>v.endsWith('%')?free*parseFloat(v)/100:v.endsWith('px')?parseFloat(v):(()=>{throw new Error('지원하지 않는 이미지 정렬입니다.');})();
   b={x:r.x+(el.clientLeft+px(s.paddingLeft)+offset(positions[0],w-iw))*sx,y:r.y+(el.clientTop+px(s.paddingTop)+offset(positions[1],h-ih))*sy,width:iw*sx,height:ih*sy};
  }
  return b;
 }
 function target(binding) {
  const el=unique(binding.selector),anchor=binding.anchorSelector?unique(binding.anchorSelector):el;
  if(binding.kind==='canvas')throw new Error('Canvas 구역의 내부 확대·이동은 아직 추적하지 못합니다. 구역 구조 확인이 필요합니다.');
  for(const [key,value] of Object.entries(binding.identity||{}))if(el.getAttribute(key)!==value)throw new Error('저장한 구역의 식별 정보가 달라졌습니다.');
  if(binding.text&&D.normalized(el.innerText||el.textContent)!==binding.text)throw new Error('저장한 구역 이름이 달라졌습니다.');
  if(binding.imageSource&&(anchor.currentSrc||anchor.src)!==binding.imageSource)throw new Error('구역 지도 이미지가 바뀌었습니다. 다시 지정하세요.');
  if(el.localName!=='area'&&!D.enabled(el))throw new Error('구역 요소가 활성 상태가 아닙니다.');
  const b=box(anchor),fraction=binding.relative||{x:.5,y:.5};
  if(!Number.isFinite(fraction.x)||!Number.isFinite(fraction.y)||fraction.x<0||fraction.x>1||fraction.y<0||fraction.y>1)throw new Error('지도 내부 비율 좌표가 올바르지 않습니다.');
  const point={x:b.x+b.width*fraction.x,y:b.y+b.height*fraction.y};
  if(point.x<0||point.y<0||point.x>=innerWidth||point.y>=innerHeight)throw new Error('선택 구역이 화면 밖입니다.');
  const hit=document.elementFromPoint(point.x,point.y);
  if(hit!==el&&!el.contains(hit)&&!(binding.anchorSelector&&(hit===anchor||anchor.contains(hit))))throw new Error('선택 구역이 가려져 있습니다.');
  return point;
 }
 function picker(kind) {
  removePicker();const banner=document.createElement('div');
  banner.style.cssText='position:fixed;left:6px;top:6px;z-index:2147483647;background:#163348;color:white;padding:10px;font:14px sans-serif;pointer-events:none';
  banner.textContent=kind==='region'?'원하는 구역을 클릭 · 위치를 저장하며 예매 클릭은 하지 않습니다 · Esc 취소':'좌석 지도 배경을 클릭 · 지도 요소를 저장합니다 · Esc 취소';document.documentElement.append(banner);
  const close=()=>{banner.remove();removeEventListener('click',pick,true);removeEventListener('keydown',key,true);};
  const key=e=>{if(e.key==='Escape')close();};
  const pick=e=>{
   e.preventDefault();e.stopImmediatePropagation();
   try{
    let el=e.target,anchor;
    if(kind==='map')el=el.closest('canvas,svg,img')||el;
    else el=el.closest('button,a,[role="button"],area')||el;
    if(el.localName==='area'){
     const map=el.closest('map');const matches=[...document.querySelectorAll('img[usemap]')].filter(n=>n.useMap==='#'+map.name);
     if(matches.length!==1)throw new Error('구역 그림을 하나로 찾을 수 없습니다.');anchor=matches[0];
    }
    const b=box(anchor||el),binding={selector:D.selector(el)};
    if(kind==='region')binding.relative={x:(e.clientX-b.x)/b.width,y:(e.clientY-b.y)/b.height};
    if(anchor)binding.anchorSelector=D.selector(anchor);
    if(kind==='region'&&el.localName==='canvas')throw new Error('이 Canvas 구역은 내부 확대·이동 구조를 확인한 뒤 지원할 수 있습니다.');
    if(kind==='region'&&(anchor||el) instanceof HTMLImageElement)binding.imageSource=(anchor||el).currentSrc||(anchor||el).src;
    if(kind==='region'){binding.identity=Object.fromEntries(['id','aria-label','title','alt','name','href','data-region','data-section','data-block','d'].filter(k=>el.hasAttribute(k)).map(k=>[k,el.getAttribute(k)]));}
    if(kind==='region'&&el.matches('button,a,[role=button]'))binding.text=D.normalized(el.innerText||el.textContent);
    binding.kind=el.localName==='canvas'?'canvas':el.localName==='img'?'image':el.localName==='area'?'image-map':'element';
    close();chrome.runtime.sendMessage({type:'seatBindingPicked',kind,binding}).catch(()=>{});
   }catch(error){banner.textContent=error.message+' · 다시 지정하거나 Esc';}
  };
  addEventListener('click',pick,true);addEventListener('keydown',key,true);removePicker=close;
 }
 function locateFrame(m) {
  if(frameRequests.size>=8)throw new Error('프레임 위치 요청이 너무 많습니다.');
  let resolve;const result=new Promise(r=>resolve=r);
  const finish=value=>{removeEventListener('message',receive);clearTimeout(timer);resolve(value);};
  const receive=e=>{
   if(e.data?.__nolFrame!==m.token)return;
   const matches=[...document.querySelectorAll('iframe,frame')].filter(f=>f.contentWindow===e.source);
   if(matches.length!==1)return;
   try{
    const el=matches[0],r=box(el),style=getComputedStyle(el),px=v=>parseFloat(v)||0;
    const sx=r.width/el.offsetWidth,sy=r.height/el.offsetHeight;
    const width=el.clientWidth-px(style.paddingLeft)-px(style.paddingRight),height=el.clientHeight-px(style.paddingTop)-px(style.paddingBottom);
    const point={x:r.x+(el.clientLeft+px(style.paddingLeft)+m.point.x*width/m.viewport.width)*sx,y:r.y+(el.clientTop+px(style.paddingTop)+m.point.y*height/m.viewport.height)*sy};
    if(m.hitCheck&&(point.x<0||point.y<0||point.x>=innerWidth||point.y>=innerHeight||document.elementFromPoint(point.x,point.y)!==el))throw new Error('구역 프레임이 가려지거나 화면 밖입니다.');
    finish({ok:true,point,viewport:{width:innerWidth,height:innerHeight}});
   }catch(error){finish({ok:false,error:error.message});}
  };
  const timer=setTimeout(()=>{finish({ok:false,error:'프레임 위치 확인 시간이 초과되었습니다.'});frameRequests.delete(m.token);},1500);
  frameRequests.set(m.token,{result,cancel:()=>finish({ok:false,error:'중지됨'})});addEventListener('message',receive);
 }
 chrome.runtime.onMessage.addListener((m,sender,reply)=>{
  try{
   if(m.type==='seatFrameLocate'){locateFrame(m);reply({ok:true});}
   else if(m.type==='seatFrameProbe'){parent.postMessage({__nolFrame:m.token},'*');reply({ok:true});}
   else if(m.type==='seatFrameResult'){
    const pending=frameRequests.get(m.token);if(!pending)throw new Error('프레임 위치 요청이 만료되었습니다.');
    pending.result.then(value=>{frameRequests.delete(m.token);reply(value);});return true;
   }
   else if(m.type==='seatBindingPoint')reply({ok:true,point:target(m.binding),viewport:{width:innerWidth,height:innerHeight}});
   else if(m.type==='seatBindingRect'){
    const rect=box(unique(m.binding.selector));const x=Math.max(0,rect.x),y=Math.max(0,rect.y),right=Math.min(innerWidth,rect.x+rect.width),bottom=Math.min(innerHeight,rect.y+rect.height);
    if(right<=x||bottom<=y)throw new Error('좌석 지도 영역이 화면 밖입니다.');
    reply({ok:true,rect:{x,y,width:right-x,height:bottom-y},viewport:{width:innerWidth,height:innerHeight}});
   }else if(m.type==='seatPickBinding'){picker(m.kind);reply({ok:true});}
   else if(m.type==='seatClear'){removePicker();for(const pending of frameRequests.values())pending.cancel();frameRequests.clear();reply({ok:true});}
  }catch(e){reply({ok:false,error:e.message});}
 });
})();

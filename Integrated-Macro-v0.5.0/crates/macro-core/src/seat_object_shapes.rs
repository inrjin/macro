//! Repeated silhouettes are learned from opaque background-separated objects.
//! Templates use the captured scale with bounded rasterization fitting at edges.
//! No hole filling, closing, dilation or shape-name classifier is applied to pixels.
use super::*;
use std::{collections::{BTreeSet, VecDeque}, sync::Arc};

const MAX_SIDE: usize = 128;
const MIN_REFERENCES: usize = 6;
const MAX_FAMILIES: usize = 32;
const MAX_COMPONENT_SEARCH: usize = 100_000;
const MAX_FRAME_SEARCH: usize = 500_000;

struct Component {
    bounds: Rect,
    pixels: Vec<(usize, usize)>,
    target_pixels: usize,
    total_pixels: usize,
}
struct Template {
    shape: Arc<SeatShape>,
    inside: Vec<(i64,i64)>,
    core: Vec<(i64,i64)>,
    outside: Vec<(i64,i64)>,
    references: usize,
    family: usize,
}
struct Proposal { family: usize, bounds: Rect, score: usize }
struct Analysis {
    target: usize, core_target: usize, core: usize, holes: usize,
    unobserved: usize, click: Option<Point>, score: usize,
}

// Lazy search-only foreground integral. The rectangle count is an exact upper
// bound for occupied template pixels, so this rejects only impossible fits.
// Ordinary separate seats take the exact-component path without allocating it.
struct ForegroundIntegral { left:usize,top:usize,width:usize,height:usize,sums:Vec<u32> }
impl ForegroundIntegral {
    fn build(frame:RgbaView<'_>,bounds:Rect,bg:Color,config:&VisionConfig)->Option<Self>{
        let left=bounds.x as usize;let top=bounds.y as usize;
        let width=bounds.width as usize;let height=bounds.height as usize;
        let area=width.checked_mul(height)?;if area>u32::MAX as usize{return None;}
        let columns=width.checked_add(1)?;let rows=height.checked_add(1)?;
        let mut sums=vec![0_u32;columns.checked_mul(rows)?];
        for y in 0..height{let mut row_sum=0_u32;for x in 0..width{
            let point=Point{x:(left+x)as f64,y:(top+y)as f64};
            if allowed(point,config)&&pixel(frame,(left+x)as i64,(top+y)as i64).is_some_and(|c|contrast(c,bg)>=6){row_sum+=1;}
            sums[(y+1)*columns+x+1]=sums[y*columns+x+1]+row_sum;
        }}
        Some(Self{left,top,width,height,sums})
    }
    fn count(&self,bounds:Rect)->Option<usize>{
        let x=(bounds.x as usize).checked_sub(self.left)?;let y=(bounds.y as usize).checked_sub(self.top)?;
        let right=x.checked_add(bounds.width as usize)?;let bottom=y.checked_add(bounds.height as usize)?;
        if right>self.width||bottom>self.height{return None;}
        let stride=self.width+1;
        Some((self.sums[bottom*stride+right]as u64+self.sums[y*stride+x]as u64
            -self.sums[y*stride+right]as u64-self.sums[bottom*stride+x]as u64)as usize)
    }
}

fn components(frame: RgbaView<'_>, area:(usize,usize,usize,usize), bg:Color,
    color:Color, tolerance:u8, config:&VisionConfig) -> Result<Vec<Component>,ValidationError> {
    let (x0,y0,x1,y1)=area;let width=x1-x0;let height=y1-y0;
    let mut mask=vec![false;width*height];let mut sampled=0;let mut opaque=0;
    for y in 0..height {for x in 0..width {
        let point=Point{x:(x+x0) as f64,y:(y+y0) as f64};
        if !allowed(point,config) {continue;} sampled+=1;
        if let Some(c)=pixel(frame,(x+x0) as i64,(y+y0) as i64) {
            opaque+=1;mask[y*width+x]=contrast(c,bg)>=6;
        }
    }}
    if sampled>0&&opaque*100<sampled*98 {
        return Err(ValidationError("[capture_incomplete] 좌석 객체 화면에 불투명하지 않은 픽셀이 너무 많습니다".into()));
    }
    let mut result=vec![];let mut queue=VecDeque::new();
    for start in 0..mask.len() {
        if !mask[start] {continue;}mask[start]=false;queue.push_back(start);
        let (mut left,mut top,mut right,mut bottom)=(width,height,0,0);
        let mut pixels=vec![];let mut target_pixels=0;let mut total_pixels=0;let mut retain=true;
        while let Some(index)=queue.pop_front() {
            let x=index%width;let y=index/width;
            left=left.min(x);right=right.max(x);top=top.min(y);bottom=bottom.max(y);
            total_pixels+=1;
            if retain && (right-left+1>MAX_SIDE || bottom-top+1>MAX_SIDE
                || total_pixels>config.max_component_pixels.min(MAX_SIDE*MAX_SIDE)) {
                retain=false;pixels.clear();pixels.shrink_to_fit();
            }
            if retain {pixels.push((x+x0,y+y0));}
            if pixel(frame,(x+x0)as i64,(y+y0)as i64).is_some_and(|c|color_matches(c,color,tolerance)){target_pixels+=1;}
            for next in [x.checked_sub(1).map(|_|index-1),(x+1<width).then_some(index+1),
                y.checked_sub(1).map(|_|index-width),(y+1<height).then_some(index+width)].into_iter().flatten(){
                if mask[next]{mask[next]=false;queue.push_back(next);}
            }
        }
        // Large merged components remain search seeds. max_component_pixels
        // bounds each reference object, never the entire connected map seed.
        if total_pixels>=4 {result.push(Component{bounds:Rect{x:(left+x0)as f64,y:(top+y0)as f64,
            width:(right-left+1)as f64,height:(bottom-top+1)as f64},pixels,target_pixels,total_pixels});}
    }
    Ok(result)
}

fn has_enclosed_hole(shape:&SeatShape)->bool{
    let w=shape.width()as usize;let h=shape.height()as usize;
    let mut seen=vec![false;w*h];let mut queue=VecDeque::new();
    for y in 0..h{for x in 0..w{if (x==0||y==0||x+1==w||y+1==h)&&!shape.occupied(x as i64,y as i64){seen[y*w+x]=true;queue.push_back((x,y));}}}
    while let Some((x,y))=queue.pop_front(){
        for (nx,ny) in [(x as i64-1,y as i64),(x as i64+1,y as i64),(x as i64,y as i64-1),(x as i64,y as i64+1)]{
            if nx>=0&&ny>=0&&nx<w as i64&&ny<h as i64{
                let i=ny as usize*w+nx as usize;
                if !seen[i]&&!shape.occupied(nx,ny){seen[i]=true;queue.push_back((nx as usize,ny as usize));}
            }
        }
    }
    (0..h).any(|y|(0..w).any(|x|!shape.occupied(x as i64,y as i64)&&!seen[y*w+x]))
}

fn template(shape:Arc<SeatShape>,references:usize)->Option<Template>{
    let w=shape.width() as usize;let h=shape.height() as usize;
    let mut inside=vec![];let mut outside=BTreeSet::new();
    let mut distance=vec![usize::MAX;w*h];let mut queue=VecDeque::new();
    for y in 0..h {for x in 0..w {if shape.occupied(x as i64,y as i64){
        inside.push((x as i64,y as i64));
        for (dx,dy) in [(-1,0),(1,0),(0,-1),(0,1)]{
            let neighbor=(x as i64+dx,y as i64+dy);
            if !shape.occupied(neighbor.0,neighbor.1){
                outside.insert(neighbor);distance[y*w+x]=1;
            }
        }
        if distance[y*w+x]==1{queue.push_back((x,y));}
    }}}
    while let Some((x,y))=queue.pop_front(){
        for (nx,ny) in [(x as i64-1,y as i64),(x as i64+1,y as i64),(x as i64,y as i64-1),(x as i64,y as i64+1)]{
            if shape.occupied(nx,ny){let i=ny as usize*w+nx as usize;
                if distance[i]==usize::MAX{distance[i]=distance[y*w+x]+1;queue.push_back((nx as usize,ny as usize));}
            }
        }
    }
    let max_depth=inside.iter().map(|&(x,y)|distance[y as usize*w+x as usize]).max()?;
    let depth=(w.min(h)/5).max(1).min(max_depth/2);
    let core=inside.iter().copied().filter(|&(x,y)|distance[y as usize*w+x as usize]>depth).collect::<Vec<_>>();
    if core.len()<4||outside.is_empty(){return None;}
    Some(Template{shape,inside,core,outside:outside.into_iter().collect(),references,family:0})
}

fn local_background(frame:RgbaView<'_>,t:&Template,b:Rect,config:&VisionConfig)->Option<Color>{
    let mut channels=[vec![],vec![],vec![]];
    for &(x,y) in &t.outside {
        let point=Point{x:b.x+x as f64,y:b.y+y as f64};
        if !allowed(point,config){return None;}
        let c=pixel(frame,point.x as i64,point.y as i64)?;
        channels[0].push(c.r);channels[1].push(c.g);channels[2].push(c.b);
    }
    let i=t.outside.len()/2;for channel in &mut channels{channel.sort_unstable();}
    Some(Color{r:channels[0][i],g:channels[1][i],b:channels[2][i]})
}

fn boundary_observation(frame:RgbaView<'_>,t:&Template,b:Rect,bg:Color,config:&VisionConfig)->Option<usize>{
    let mut blocked=vec![];
    for &(x,y) in &t.outside {
        let p=Point{x:b.x+x as f64,y:b.y+y as f64};
        if !allowed(p,config){return None;}
        let c=pixel(frame,p.x as i64,p.y as i64)?;
        if contrast(c,bg)>=6{blocked.push((x,y));if blocked.len()>2.max(t.outside.len()/12){return None;}}
    }
    if blocked.len()>2.max(t.outside.len()/12){return None;}
    // Permit isolated 1-pixel bridges. A broad connected interruption is not
    // enough evidence to split a merged object, even if its total area is small.
    while let Some(first)=blocked.pop(){
        let mut connected=vec![first];let mut cursor=0;
        while cursor<connected.len(){
            let (x,y)=connected[cursor];cursor+=1;
            let mut i=0;while i<blocked.len(){
                if (blocked[i].0-x).abs()<=1&&(blocked[i].1-y).abs()<=1 {
                    connected.push(blocked.swap_remove(i));
                }else{i+=1;}
            }
        }
        if connected.len()>2{return None;}
    }
    let mut visible=0;
    for &(x,y) in &t.inside{
        let p=Point{x:b.x+x as f64,y:b.y+y as f64};
        if !allowed(p,config){return None;}
        let c=pixel(frame,p.x as i64,p.y as i64)?;
        if contrast(c,bg)>=6{visible+=1;}
    }
    Some(visible)
}
fn boundary_score(frame:RgbaView<'_>,t:&Template,b:Rect,bg:Color,config:&VisionConfig)->Option<usize>{
    let visible=boundary_observation(frame,t,b,bg,config)?;
    (visible*100>=t.inside.len()*90).then_some(visible)
}

fn analyze(frame:RgbaView<'_>,t:&Template,b:Rect,color:Color,tolerance:u8,bg:Color,
    config:&VisionConfig)->Analysis{
    let mut a=Analysis{target:0,core_target:0,core:t.core.len(),holes:0,unobserved:0,click:None,score:0};
    let center=Point{x:b.x+(b.width-1.0)/2.0,y:b.y+(b.height-1.0)/2.0};let mut distance=f64::INFINITY;
    for &(x,y) in &t.inside{
        let p=Point{x:b.x+x as f64,y:b.y+y as f64};
        let Some(c)=allowed(p,config).then(||pixel(frame,p.x as i64,p.y as i64)).flatten()else{a.unobserved+=1;continue;};
        if color_matches(c,color,tolerance){
            a.target+=1;let d=(p.x-center.x).powi(2)+(p.y-center.y).powi(2);
            if d<distance&&seat_point_supported(frame,p,color,tolerance,config){a.click=Some(p);distance=d;}
        }
    }
    for &(x,y) in &t.core{
        let p=Point{x:b.x+x as f64,y:b.y+y as f64};
        if let Some(c)=allowed(p,config).then(||pixel(frame,p.x as i64,p.y as i64)).flatten(){
            a.core_target+=usize::from(color_matches(c,color,tolerance));a.holes+=usize::from(contrast(c,bg)<6);
        }
    }
    a.score=boundary_score(frame,t,b,bg,config).unwrap_or(0);a
}

fn learn(frame:RgbaView<'_>,components:&[Component],bg:Color,config:&VisionConfig)->Vec<Template>{
    let mut families:BTreeMap<(u32,u32,u64),Template>=BTreeMap::new();
    for c in components{
        let w=c.bounds.width as usize;let h=c.bounds.height as usize;
        if w<5||h<5||w>MAX_SIDE||h>MAX_SIDE||c.pixels.is_empty()||c.total_pixels>config.max_component_pixels{continue;}
        let mut mask=vec![false;w*h];
        for &(x,y) in &c.pixels{mask[(y-c.bounds.y as usize)*w+x-c.bounds.x as usize]=true;}
        let Some(shape)=SeatShape::from_occupancy(w as u32,h as u32,&mask)else{continue;};
        let key=(w as u32,h as u32,shape.fingerprint());
        if let Some(t)=families.get_mut(&key){
            // Identical masks reuse their topology/core arrays. Validation of
            // this reference's pixels and surroundings still runs every time.
            if *t.shape==shape && boundary_score(frame,t,c.bounds,bg,config).is_some(){t.references+=1;}
            continue;
        }
        // Repeated interior background marks may be selected checks. They
        // cannot define a new family that removes holes from the denominator.
        if has_enclosed_hole(&shape){continue;}
        let shape=Arc::new(shape);
        let Some(t)=template(shape,1)else{continue;};
        if boundary_score(frame,&t,c.bounds,bg,config).is_some(){families.insert(key,t);}
    }
    let mut templates=families.into_values().filter(|t|t.references>=MIN_REFERENCES).collect::<Vec<_>>();
    templates.sort_by_key(|t|std::cmp::Reverse(t.references));templates.truncate(MAX_FAMILIES);
    let composite=(0..templates.len()).map(|i|decomposable(&templates[i],&templates)).collect::<Vec<_>>();
    templates.into_iter().enumerate().filter_map(|(i,mut t)|{
        if composite[i]{None}else{t.family=i;Some(t)}
    }).collect()
}

// Repeated connected pairs do not become a larger single-seat family when two
// independently repeated, disjoint silhouettes already explain the same mask.
// A wide merge does not pass the small-template surrounding-boundary test.
fn decomposable(large:&Template,all:&[Template])->bool{
    let mut parts:Vec<(Rect,&SeatShape)>=vec![];
    let w=large.shape.width()as i64;let h=large.shape.height()as i64;
    for small in all {
        if small.shape.pixel_count()*100>=large.shape.pixel_count()*70
            ||small.shape.width()>large.shape.width()||small.shape.height()>large.shape.height(){continue;}
        for y in 0..=h-small.shape.height()as i64{for x in 0..=w-small.shape.width()as i64{
            let blocked=small.outside.iter().filter(|&&(dx,dy)|large.shape.occupied(x+dx,y+dy)).take(3).count();
            if blocked>2{continue;}
            if small.inside.iter().filter(|&&(dx,dy)|large.shape.occupied(x+dx,y+dy)).count()*100<small.inside.len()*95{continue;}
            let b=Rect{x:x as f64,y:y as f64,width:small.shape.width()as f64,height:small.shape.height()as f64};
            if parts.iter().any(|(pb,ps)|small.shape.overlap_pixels(b,ps,*pb)>0){continue;}
            parts.push((b,&small.shape));
            if parts.len()>=2&&parts.iter().map(|(_,s)|s.pixel_count()).sum::<usize>()*100>=large.shape.pixel_count()*80{return true;}
        }}
    }
    false
}

// Fit only the observed edge of a whole component to a repeated silhouette.
// Internal missing pixels stay in the expected mask, so text/check holes remain
// visible to state validation instead of silently becoming a smaller object.
fn fit_component(t:&Template,c:&Component)->Option<Template>{
    let w=c.bounds.width as usize;let h=c.bounds.height as usize;
    if c.pixels.is_empty() || (w as i64-t.shape.width() as i64).abs()>2
        || (h as i64-t.shape.height() as i64).abs()>2 {return None;}
    let mut observed=vec![false;w*h];
    for &(x,y) in &c.pixels{observed[(y-c.bounds.y as usize)*w+x-c.bounds.x as usize]=true;}
    let mut expected=vec![false;w*h];
    for y in 0..h{for x in 0..w{
        let sx=((x as f64+0.5)*t.shape.width()as f64/w as f64).floor()as i64;
        let sy=((y as f64+0.5)*t.shape.height()as f64/h as f64).floor()as i64;
        expected[y*w+x]=t.shape.occupied(sx,sy);
    }}
    // Rasterized perimeter changes scale with perimeter rather than area: a
    // diamond can gain an entire 1-pixel AA edge without a meaningful outline
    // change. Validate the local edge band and unchanged interior below rather
    // than applying one global intersection ratio to all sizes/shapes.
    let at=|mask:&[bool],x:i64,y:i64|x>=0&&y>=0&&x<w as i64&&y<h as i64&&mask[y as usize*w+x as usize];
    let mut fitted=expected.clone();
    for y in 0..h{for x in 0..w{
        let i=y*w+x;if expected[i]==observed[i]{continue;}
        // A one-pixel rasterization change must border both foreground and
        // background in the normalized learned mask. Deep additions are not
        // inferred from color; deep holes remain expected occupied evidence.
        let mut fg=false;let mut bg=false;
        for dy in -1..=1{for dx in -1..=1{
            if at(&expected,x as i64+dx,y as i64+dy){fg=true;}else{bg=true;}
        }}
        if fg&&bg{fitted[i]=observed[i];}
        else if observed[i]{return None;}
    }}
    let shape=Arc::new(SeatShape::from_occupancy(w as u32,h as u32,&fitted)?);
    let mut fitted=template(shape,t.references)?;fitted.family=t.family;Some(fitted)
}

// Two disconnected colored pieces may fit a larger independently repeated
// outline interrupted by a background-colored mark. Keep that competing whole
// hypothesis local: it cannot authorize a click, but it prevents learning the
// same marked object as two new seats. Identical pixels depicting two real small
// seats remain ambiguous; unrelated small objects are unaffected.
fn fragment_competition(frame:RgbaView<'_>,proposals:&[Proposal],templates:&[Template],
    learned_count:usize,bg:Color,color:Color,tolerance:u8,config:&VisionConfig)->BTreeSet<usize>{
    let mut ambiguous=BTreeSet::new();
    let mut order=(0..proposals.len()).collect::<Vec<_>>();
    order.sort_by(|&a,&b|proposals[a].bounds.x.total_cmp(&proposals[b].bounds.x));
    for (position,&i) in order.iter().enumerate(){
        let a=&proposals[i];let ashape=&templates[a.family].shape;
        for &j in &order[position+1..]{
            let b=&proposals[j];if b.bounds.x-a.bounds.x>MAX_SIDE as f64{break;}
            if (b.bounds.y-a.bounds.y).abs()>MAX_SIDE as f64{continue;}
            let bshape=&templates[b.family].shape;
            if ashape.overlap_pixels(a.bounds,bshape,b.bounds)>0{continue;}
            let left=a.bounds.x.min(b.bounds.x);let top=a.bounds.y.min(b.bounds.y);
            let width=(a.bounds.x+a.bounds.width).max(b.bounds.x+b.bounds.width)-left;
            let height=(a.bounds.y+a.bounds.height).max(b.bounds.y+b.bounds.height)-top;
            for whole in &templates[..learned_count]{
                if whole.shape.pixel_count()*100<ashape.pixel_count().max(bshape.pixel_count())*125
                    ||width>whole.shape.width()as f64+2.0||height>whole.shape.height()as f64+2.0
                    ||width<whole.shape.width()as f64*0.60||height<whole.shape.height()as f64*0.60
                    ||((width-whole.shape.width()as f64).abs()>2.0
                        &&(height-whole.shape.height()as f64).abs()>2.0){continue;}
                let bounds=Rect{x:(left+(width-whole.shape.width()as f64)/2.0).round(),
                    y:(top+(height-whole.shape.height()as f64)/2.0).round(),
                    width:whole.shape.width()as f64,height:whole.shape.height()as f64};
                if whole.shape.overlap_pixels(bounds,ashape,a.bounds)*100<ashape.pixel_count()*90
                    ||whole.shape.overlap_pixels(bounds,bshape,b.bounds)*100<bshape.pixel_count()*90{continue;}
                let Some(visible)=boundary_observation(frame,whole,bounds,bg,config)else{continue;};
                if visible*100<whole.inside.len()*60{continue;}
                let state=analyze(frame,whole,bounds,color,tolerance,bg,config);
                if state.unobserved==0&&state.holes*100>state.core*8{
                    // The competing whole may contain three or more pieces;
                    // marking just the triggering pair would leave its middle
                    // fragment clickable. Hold every contained proposal in it.
                    for (k,piece) in proposals.iter().enumerate(){
                        if piece.bounds.x+piece.bounds.width<=bounds.x||piece.bounds.x>=bounds.x+bounds.width
                            ||piece.bounds.y+piece.bounds.height<=bounds.y||piece.bounds.y>=bounds.y+bounds.height{continue;}
                        let shape=&templates[piece.family].shape;
                        if whole.shape.overlap_pixels(bounds,shape,piece.bounds)*100>=shape.pixel_count()*90{ambiguous.insert(k);}
                    }
                    break;
                }
            }
        }
    }
    ambiguous
}

fn observation_context(frame:RgbaView<'_>,color:Color,tolerance:u8,config:&VisionConfig)->Option<SeatObjectContext>{
    if frame.stride<(frame.width as usize).checked_mul(4)?
        ||frame.rgba.len()<frame.stride.checked_mul(frame.height as usize)?{return None;}
    let (x0,y0,x1,y1)=roi(frame,config);let mut digest=14695981039346656037_u64;
    for y in y0..y1{
        let row=frame.rgba.get(y*frame.stride+x0*4..y*frame.stride+x1*4)?;
        let mut chunks=row.chunks_exact(8);
        for chunk in &mut chunks{digest=(digest^u64::from_le_bytes(chunk.try_into().ok()?)).wrapping_mul(1099511628211);}
        for &byte in chunks.remainder(){digest=(digest^byte as u64).wrapping_mul(1099511628211);}
    }
    Some(SeatObjectContext{width:frame.width,height:frame.height,stride:frame.stride,digest,
        roi:config.roi,exclusions:config.exclusions.clone(),min_component_pixels:config.min_component_pixels,
        max_component_pixels:config.max_component_pixels,color,tolerance})
}

/// The exact learned silhouette travels with the candidate in memory. Missing
/// proof (including after JSON serialization) is rejected, never guessed round.
pub fn seat_object_candidate_supported(frame:RgbaView<'_>,candidate:&Candidate,color:Color,tolerance:u8,config:&VisionConfig)->bool{
    let Some(g)=candidate.geometry.as_ref()else{return false;};
    let Some(shape)=g.object_shape.as_ref()else{return false;};
    let Some(proof)=g.object_context.as_ref()else{return false;};
    let Some(current)=observation_context(frame,color,tolerance,config)else{return false;};
    if **proof!=current {
        // Full interpretation is re-evaluated once on this same acquired frame.
        // No capture loop, and no raw candidate-id assumption: the runtime may
        // prefix cancellation candidates with their region identity.
        let Ok(latest)=extract_seat_objects_view_with_stats(frame,color,tolerance,config,0)else{return false;};
        if !latest.candidates.iter().any(|candidate|candidate.geometry.as_ref().is_some_and(|new|
            new.bounds==g.bounds&&new.object_shape.as_ref().is_some_and(|new_shape|**new_shape==**shape))){return false;}
    }
    if !shape.valid_at(g.bounds)||!shape.contains_at(g.bounds,candidate.point)
        ||!seat_point_supported(frame,candidate.point,color,tolerance,config){return false;}
    let Some(t)=template(shape.clone(),0)else{return false;};
    let Some(bg)=local_background(frame,&t,g.bounds,config)else{return false;};
    let a=analyze(frame,&t,g.bounds,color,tolerance,bg,config);
    a.score>0&&a.unobserved==0&&a.core>0&&a.core_target*100>=a.core*45&&a.holes*100<=a.core*8
}

fn unresolved(output:&mut SeatObjectExtraction,c:&Component,reason:ObjectReason){
    if c.target_pixels==0{return;}
    output.object_stats.ambiguous_objects+=1;
    output.objects.push(SeatObjectEvidence{bounds:c.bounds,center:Point{x:c.bounds.x+c.bounds.width/2.0,y:c.bounds.y+c.bounds.height/2.0},
        reason,target_pixels:c.target_pixels,core_pixels:0,core_target_pixels:0,core_background_pixels:0,core_unobserved_pixels:0,source_components:1});
}

pub fn extract_seat_objects_view_with_stats(frame:RgbaView<'_>,color:Color,tolerance:u8,config:&VisionConfig,generation:u64)->Result<SeatObjectExtraction,ValidationError>{
    let seeds=extract_candidates_view_with_stats(frame,color,tolerance,config,generation)?;
    let mut output=SeatObjectExtraction{candidates:vec![],stats:seeds.stats,object_stats:SeatObjectStats::default(),objects:vec![]};
    output.stats.accepted_count=0;let area=roi(frame,config);
    if area.0==area.2||area.1==area.3{return Ok(output);}
    let Some((bg,samples))=background(frame,area,config)else{
        for seed in &seeds.candidates{if let Some(g)=seed.geometry.as_ref(){
            unresolved(&mut output,&Component{bounds:g.bounds,pixels:vec![],target_pixels:g.pixels,total_pixels:g.pixels},ObjectReason::BackgroundUncertain);
        }}return Ok(output);
    };
    output.object_stats.background=Some(bg);output.object_stats.background_samples=samples;
    let components=components(frame,area,bg,color,tolerance,config)?;
    output.object_stats.foreground_components=components.len();
    let mut templates=learn(frame,&components,bg,config);
    let learned_count=templates.len();
    output.object_stats.template_families=templates.len();
    output.object_stats.reference_objects=templates.iter().map(|t|t.references).sum();
    if templates.is_empty(){for c in &components{unresolved(&mut output,c,ObjectReason::NoRepeatedShape);}return Ok(output);}
    if templates.len()==1{output.object_stats.template_diameter=Some((templates[0].shape.width()+templates[0].shape.height())as f64/2.0);}
    let mut proposals=vec![];let mut searched=0_usize;
    let mut variants:BTreeMap<(usize,u32,u32,u64),usize>=BTreeMap::new();
    for c in components.iter().filter(|c|c.target_pixels>0){
        let mut local=vec![];let mut budget=false;let mut integral=None;
        for family in 0..learned_count {
            let t=&templates[family];
            let exact=c.bounds.width==t.shape.width()as f64&&c.bounds.height==t.shape.height()as f64
                &&c.pixels.len()==t.shape.pixel_count()&&c.pixels.iter().all(|&(x,y)|t.shape.occupied(x as i64-c.bounds.x as i64,y as i64-c.bounds.y as i64));
            if exact {
                if let Some(score)=boundary_score(frame,t,c.bounds,bg,config){
                    local.push(Proposal{family,bounds:c.bounds,score});continue;
                }
            } else if let Some(fitted)=fit_component(t,c){
                if let Some(score)=boundary_score(frame,&fitted,c.bounds,bg,config){
                    let key=(family,fitted.shape.width(),fitted.shape.height(),fitted.shape.fingerprint());
                    let index=if let Some(&index)=variants.get(&key){
                        if *templates[index].shape!=*fitted.shape{continue;}index
                    }else{let index=templates.len();templates.push(fitted);variants.insert(key,index);index};
                    local.push(Proposal{family:index,bounds:c.bounds,score});continue;
                }
            }
            let t=&templates[family];
            let w=t.shape.width()as i64;let h=t.shape.height()as i64;
            if c.bounds.width<w as f64||c.bounds.height<h as f64{continue;}
            let left=c.bounds.x as i64;let top=c.bounds.y as i64;
            let right=left+c.bounds.width as i64-w;let bottom=top+c.bounds.height as i64-h;
            let count=((right-left+1)*(bottom-top+1))as usize;
            if count>MAX_COMPONENT_SEARCH||searched.saturating_add(count)>MAX_FRAME_SEARCH{budget=true;continue;}
            searched+=count;
            if count>128&&integral.is_none(){integral=ForegroundIntegral::build(frame,c.bounds,bg,config);}
            for y in top..=bottom{for x in left..=right{
                let b=Rect{x:x as f64,y:y as f64,width:w as f64,height:h as f64};
                if integral.as_ref().and_then(|sum|sum.count(b)).is_some_and(|visible|visible*100<t.inside.len()*90){continue;}
                if let Some(score)=boundary_score(frame,t,b,bg,config){local.push(Proposal{family,bounds:b,score});}
            }}
        }
        if budget{output.object_stats.search_budget_exhausted+=1;}
        if local.is_empty(){unresolved(&mut output,c,ObjectReason::UnsupportedShape);}
        local.sort_by(|a,b|b.score.cmp(&a.score).then_with(||a.bounds.y.total_cmp(&b.bounds.y)).then_with(||a.bounds.x.total_cmp(&b.bounds.x)));
        for proposal in local{
            let shape=&templates[proposal.family].shape;
            // Suppress shifted fits of the same occupied image. Unlike a fixed
            // radius, disjoint close silhouettes never suppress one another.
            if proposals.iter().any(|other:&Proposal|{
                let other_shape=&templates[other.family].shape;
                shape.overlap_pixels(proposal.bounds,other_shape,other.bounds)*100>=shape.pixel_count().min(other_shape.pixel_count())*75
            }){continue;}
            proposals.push(proposal);
        }
    }
    let fragment_ambiguity=fragment_competition(frame,&proposals,&templates,learned_count,bg,color,tolerance,config);
    let context=Arc::new(observation_context(frame,color,tolerance,config).ok_or_else(||ValidationError("[capture_incomplete] 좌석 객체 문맥을 읽지 못했습니다".into()))?);
    let mut outline_families=BTreeSet::new();let mut available=vec![];
    for (index,p) in proposals.iter().enumerate(){
        let t=&templates[p.family];let a=analyze(frame,t,p.bounds,color,tolerance,bg,config);
        if a.target==0{continue;}
        let overlaps=proposals.iter().enumerate().any(|(i,other)|i!=index
            &&t.shape.overlap_pixels(p.bounds,&templates[other.family].shape,other.bounds)>0);
        let members=seeds.candidates.iter().filter(|s|t.shape.contains_at(p.bounds,s.point)).count();
        let reason=if a.unobserved>0{ObjectReason::IncompleteObject}
            else if overlaps||fragment_ambiguity.contains(&index){ObjectReason::OverlappingHypotheses}
            else if a.core==0||a.core_target*100<a.core*45{outline_families.insert(t.family);ObjectReason::OutlineOnlyColor}
            else if a.holes*100>a.core*8{ObjectReason::InternalBackgroundMark}
            else if a.score==0||a.click.is_none(){ObjectReason::UnsupportedShape}
            else{ObjectReason::AvailableFill};
        if matches!(reason,ObjectReason::AvailableFill){available.push((t.family,output.objects.len(),Candidate{
            id:format!("object:{}:{}:{}",p.bounds.x,p.bounds.y,t.shape.fingerprint()),point:a.click.unwrap(),color,
            target:Target::Point{point:a.click.unwrap()},selected:false,
            geometry:Some(CandidateGeometry{bounds:p.bounds,pixels:a.target,object_shape:Some(t.shape.clone()),object_context:Some(context.clone())})}));
            output.object_stats.merged_color_components+=members.saturating_sub(1);
        }else{output.object_stats.ambiguous_objects+=1;}
        output.objects.push(SeatObjectEvidence{bounds:p.bounds,center:Point{x:p.bounds.x+(p.bounds.width-1.0)/2.0,y:p.bounds.y+(p.bounds.height-1.0)/2.0},
            reason,target_pixels:a.target,core_pixels:a.core,core_target_pixels:a.core_target,core_background_pixels:a.holes,
            core_unobserved_pixels:a.unobserved,source_components:members});
    }
    for (family,index,candidate) in available{
        if outline_families.contains(&family){output.objects[index].reason=ObjectReason::ColorUsedForFillAndOutline;output.object_stats.ambiguous_objects+=1;}
        else{output.candidates.push(candidate);}
    }
    output.object_stats.resolved_objects=output.candidates.len();
    output.object_stats.split_connected_components=components.iter().filter(|c|c.target_pixels>0
        &&output.candidates.iter().filter(|candidate|c.bounds.contains(candidate.point)).count()>1).count();
    rank_seats(&mut output.candidates);output.stats.accepted_count=output.candidates.len();Ok(output)
}

struct Box2 {x:usize,y:usize,w:usize,h:usize}
struct Template {width:usize,height:usize,mask:Vec<bool>,ink:usize}
fn score(mask: &[bool], width: usize, height: usize, b: Box2, t: &Template) -> f64 {
    score_at_least(mask,width,height,b,t,0.0)
}

// Reject only when even perfect agreement on all remaining expected ink cannot
// reach the requested score. This avoids fully scoring thousands of unrelated
// seat-map components without weakening recognition thresholds.
fn score_at_least(mask: &[bool], width: usize, height: usize, b: Box2, t: &Template, minimum: f64) -> f64 {
    if b.x+b.w>width || b.y+b.h>height || b.w==0 || b.h==0 { return 0.0; }
    let mut intersection=0; let mut union=0; let mut remaining=t.ink;
    for y in 0..t.height { for x in 0..t.width {
        let px=b.x+((x*2+1)*b.w/(t.width*2)).min(b.w-1);
        let py=b.y+((y*2+1)*b.h/(t.height*2)).min(b.h-1);
        let actual=mask[py*width+px]; let expected=t.mask[y*t.width+x];
        intersection+=usize::from(actual && expected); union+=usize::from(actual || expected);
        remaining-=usize::from(expected);
    }
        if ((intersection+remaining) as f64) < minimum*(union+remaining) as f64 {return 0.0;}
    }
    if union==0 { 0.0 } else { intersection as f64/union as f64 }
}

fn main(){
 let t=Template{width:100,height:1,mask:vec![true;100],ink:100};
 let mut actual=vec![false;100];actual[..55].fill(true);
 let b=Box2{x:0,y:0,w:100,h:1};
 let exact=score(&actual,100,1,b,&t);
 let bounded=score_at_least(&actual,100,1,Box2{x:0,y:0,w:100,h:1},&t,0.55);
 println!("exact={exact:?} bounded={bounded:?} product={:?}",0.55_f64*100.0);
 assert!(exact>=0.55);
 assert_eq!(bounded,0.0,"reconstructed old multiplication comparison reproduces rejection");
}

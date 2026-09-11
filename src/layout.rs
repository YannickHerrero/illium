use crate::command::Direction;
use serde::{Deserialize, Serialize};
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }
impl Rect {
    pub fn inset(self, gap: i32) -> Self {
        let g = gap.max(0).min((self.w.min(self.h)-1).max(0)/2);
        Self { x:self.x+g, y:self.y+g, w:(self.w-2*g).max(1), h:(self.h-2*g).max(1) }
    }
}
pub fn fibonacci(area: Rect, count: usize, gap: i32, outer: i32) -> Vec<Rect> {
    let mut r = area.inset(outer);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        if i+1 == count { out.push(r); break; }
        let horizontal = i%2 == 0;
        let length = if horizontal { r.w } else { r.h };
        // Once a split is physically impossible, stack the remaining clients.
        if length < 3 { out.extend(std::iter::repeat_n(r, count-i)); break; }
        let g = gap.max(0).min(length-2);
        let half = (length-g)/2;
        let mut first = r;
        if horizontal { first.w=half; r.x+=half+g; r.w-=half+g; }
        else { first.h=half; r.y+=half+g; r.h-=half+g; }
        out.push(first);
    }
    out
}
pub fn neighbor(rects: &[(isize, Rect)], current: isize, direction: Direction) -> Option<isize> {
    let a = rects.iter().find(|(id,_)| *id==current)?.1;
    rects.iter().filter(|(id,_)| *id!=current).filter_map(|(id,b)| {
        let dx = i64::from(b.x)*2+i64::from(b.w)-i64::from(a.x)*2-i64::from(a.w);
        let dy = i64::from(b.y)*2+i64::from(b.h)-i64::from(a.y)*2-i64::from(a.h);
        let (forward, side) = match direction { Direction::Left => (-dx,dy), Direction::Right => (dx,dy), Direction::Up => (-dy,dx), Direction::Down => (dy,dx) };
        (forward>0).then_some((forward*forward+side*side*4, *id))
    }).min().map(|(_,id)| id)
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn geometry() {
        let a=Rect{x:-100,y:20,w:1200,h:800};
        assert!(fibonacci(a,0,6,6).is_empty());
        assert_eq!(fibonacci(a,1,6,6),vec![a.inset(6)]);
        for count in 1..10 { let rs=fibonacci(a,count,6,6); assert_eq!(rs.len(),count); for r in rs { assert!(r.w>0 && r.h>0); assert!(r.x>=a.x && r.y>=a.y && r.x+r.w<=a.x+a.w && r.y+r.h<=a.y+a.h); } }
    }
    #[test] fn navigation() {
        let rs = vec![(1,Rect{x:0,y:0,w:100,h:100}),(2,Rect{x:110,y:0,w:100,h:100}),(3,Rect{x:110,y:110,w:100,h:100})];
        assert_eq!(neighbor(&rs,1,Direction::Right),Some(2));
        assert_eq!(neighbor(&rs,2,Direction::Down),Some(3));
        assert_eq!(neighbor(&rs,1,Direction::Left),None);
    }
    #[test] fn tiny() { assert_eq!(fibonacci(Rect{x:0,y:0,w:1,h:1},100,100,100).len(),100); }
}

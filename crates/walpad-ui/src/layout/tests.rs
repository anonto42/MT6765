use super::*;

#[test]
fn portrait_and_landscape_frames_match_the_spec() {
    let p = Frame::new(800, 1280);
    assert_eq!(p.sidebar, Rect::new(0, 0, 96, 1280));
    assert_eq!(p.header, Rect::new(96, 0, 704, 64));
    assert_eq!(p.content, Rect::new(96, 64, 704, 1216));
    assert_eq!(p.padded(), Rect::new(120, 64, 656, 1216));
    let l = Frame::new(1280, 800);
    assert_eq!(l.content, Rect::new(96, 64, 1184, 736));
    assert_eq!(l.sidebar.h, 800);
    assert!(l.landscape() && !p.landscape());
    assert_eq!(l.screen(), Rect::new(0, 0, 1280, 800));
}

#[test]
fn columns_split_evenly() {
    let c = columns(Frame::new(800, 1280).padded(), 2, 24);
    assert_eq!((c[0].w, c[1].w, c[1].x), (316, 316, 460));
}

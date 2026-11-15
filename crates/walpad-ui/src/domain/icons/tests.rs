use super::*;
use crate::tokens::*;

#[test]
fn every_icon_draws_something_inside_its_box() {
    for icon in Icon::ALL {
        let mut c = Canvas::new(48, 48, BG).unwrap();
        c.icon(icon, 12, 12, 24, TEXT);
        let lit: Vec<(i32, i32)> = (0..48)
            .flat_map(|x| (0..48).map(move |y| (x, y)))
            .filter(|&(x, y)| c.pixel(x, y) != Some(BG))
            .collect();
        assert!(lit.len() > 15, "{icon:?} drew {} pixels", lit.len());
        // strokes may bleed 1 px past the 24 grid (round caps), nothing further
        assert!(
            lit.iter()
                .all(|&(x, y)| (11..37).contains(&x) && (11..37).contains(&y)),
            "{icon:?} drew outside its box"
        );
    }
}

#[test]
fn icons_scale() {
    let mut small = Canvas::new(64, 64, BG).unwrap();
    let mut big = Canvas::new(64, 64, BG).unwrap();
    small.icon(Icon::Wifi, 0, 0, 24, TEXT);
    big.icon(Icon::Wifi, 0, 0, 48, TEXT);
    let count = |c: &Canvas| {
        (0..64)
            .flat_map(|x| (0..64).map(move |y| (x, y)))
            .filter(|&(x, y)| c.pixel(x, y) != Some(BG))
            .count()
    };
    assert!(count(&big) > count(&small) * 2);
}

#[test]
fn names_from_apps_json() {
    assert_eq!(Icon::from_name("globe"), Some(Icon::Globe));
    assert_eq!(Icon::from_name("files"), Some(Icon::Folder));
    assert_eq!(Icon::from_name("nope"), None);
    assert_eq!(Icon::ALL.len(), 50);
    assert_eq!(Icon::from_name("phone"), Some(Icon::Phone));
    assert_eq!(Icon::from_name("bluetooth"), Some(Icon::Bluetooth));
    assert_eq!(Icon::from_name("code"), Some(Icon::Code));
    assert_eq!(Icon::from_name("editor"), Some(Icon::Code));
}

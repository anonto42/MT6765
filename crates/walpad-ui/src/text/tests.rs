use super::*;
use crate::tokens::*;

fn fonts() -> Fonts {
    Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")).unwrap()
}

#[test]
fn measures_and_caches_glyphs() {
    let mut f = fonts();
    let w1 = f.measure("WalPad", Face::Regular, 18.0);
    let n = f.cached_glyphs();
    assert!(w1 > 40.0 && w1 < 120.0, "{w1}");
    let w2 = f.measure("WalPad", Face::Regular, 18.0);
    assert_eq!(w1, w2);
    assert_eq!(f.cached_glyphs(), n, "second measure hits the cache");
    assert!(f.measure("WalPad", Face::Bold, 18.0) > w1, "bold is wider");
    assert!(
        f.measure("iiii", Face::Mono, 15.0) > f.measure("iiii", Face::Regular, 15.0),
        "mono is fixed width"
    );
}

#[test]
fn ellipsis_fits_the_box() {
    let mut f = fonts();
    let long = "A very long network name that cannot fit";
    let fitted = f.fit(long, Face::Regular, 18.0, 150.0);
    assert!(fitted.ends_with('…'));
    assert!(f.measure(&fitted, Face::Regular, 18.0) <= 150.0);
    assert_eq!(f.fit("Home", Face::Regular, 18.0, 150.0), "Home");
}

#[test]
fn drawing_changes_pixels_only_inside_the_box() {
    let mut f = fonts();
    let mut c = Canvas::new(200, 60, BG).unwrap();
    f.draw_in(
        &mut c,
        "Home",
        Style::new(Face::Regular, 18.0, TEXT),
        Rect::new(10, 10, 120, 40),
        Align::Left,
    );
    let lit = (0..200)
        .flat_map(|x| (0..60).map(move |y| (x, y)))
        .filter(|&(x, y)| c.pixel(x, y) != Some(BG))
        .collect::<Vec<_>>();
    assert!(lit.len() > 100, "some pixels drawn");
    assert!(
        lit.iter()
            .all(|&(x, y)| (10..130).contains(&x) && (10..50).contains(&y)),
        "nothing outside the box"
    );
}

#[test]
fn utf8_and_missing_glyphs_do_not_panic() {
    let mut f = fonts();
    let mut c = Canvas::new(120, 40, BG).unwrap();
    f.draw_in(
        &mut c,
        "Café ✓ 日本",
        Style::new(Face::Regular, 18.0, TEXT),
        Rect::new(0, 0, 120, 40),
        Align::Center,
    );
}

#[test]
fn missing_font_dir_is_an_error() {
    assert!(matches!(
        Fonts::load(Path::new("/nonexistent")),
        Err(FontError::Io(_))
    ));
}

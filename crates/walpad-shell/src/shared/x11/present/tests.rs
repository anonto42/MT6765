use super::*;

#[test]
fn converts_rgba_to_bgrx_for_a_sub_rect() {
    // 3x2 image, pixel (x,y) = (x*10+1, y*10+2, 3, 255)
    let mut rgba = Vec::new();
    for y in 0..2u8 {
        for x in 0..3u8 {
            rgba.extend_from_slice(&[x * 10 + 1, y * 10 + 2, 3, 255]);
        }
    }
    let out = rgba_to_bgrx(&rgba, 3, 1, 3, 1, 2);
    assert_eq!(out, vec![3, 12, 11, 0, 3, 12, 21, 0]);
    assert_eq!(rgba_to_bgrx(&rgba, 3, 0, 3, 0, 2).len(), 3 * 2 * 4);
}

#[test]
fn chunking_respects_the_request_budget() {
    assert_eq!(rows_per_chunk(800, usize::MAX), (CHUNK_BYTES - 64) / 3200);
    assert_eq!(
        rows_per_chunk(800, 10_000),
        (10_000 - 64) / 3200,
        "a small server limit wins"
    );
    assert_eq!(rows_per_chunk(100_000, 1000), 1, "always at least one row");
}

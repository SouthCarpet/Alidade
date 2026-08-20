#[test]
fn ping_and_loss_scales_include_zero() {
    let (lo, hi) = alidade_app::chart::y_scale(&[38.0, 121.0, 44.0], true);
    assert_eq!(lo, 0.0);
    assert!(hi >= 121.0);
}

#[test]
fn throughput_scale_does_not_pad_a_flat_line_into_noise() {
    // a flat 312 Mbit/s line must not be rendered as a wild zigzag by an
    // over-tight scale: the band must be at least 10% of the value
    let (lo, hi) = alidade_app::chart::y_scale(&[312.0, 312.4, 311.8], false);
    assert!(hi - lo >= 31.0, "flat series got a hair-thin scale: {lo}..{hi}");
}

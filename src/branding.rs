pub fn spark_icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let center = (s - 1.0) / 2.0;
    let half = s * 0.47;
    let corner_radius = s * 0.20;
    let ring_width = s * 0.018;
    let glow_radius = s * 0.30;
    let core_radius = s * 0.068;

    let bg_outer = (34_u8, 40, 52);
    let bg_inner = (52_u8, 66, 88);
    let rim_color = (100_u8, 122, 154);
    let spark_color = (212_u8, 236, 255);
    let spark_core = (244_u8, 250, 255);

    let mut rgba = vec![0_u8; (size * size * 4) as usize];

    for y in 0..size {
        for x in 0..size {
            let fx = x as f32 - center;
            let fy = y as f32 - center;
            let ax = fx.abs();
            let ay = fy.abs();

            let inner_half = half - corner_radius;
            let dx = (ax - inner_half).max(0.0);
            let dy = (ay - inner_half).max(0.0);
            let sdf = (dx * dx + dy * dy).sqrt() - corner_radius;

            let outside_edge = 0.5;
            if sdf > outside_edge {
                continue;
            }

            let distance = (fx * fx + fy * fy).sqrt();
            let radial = (distance / half).clamp(0.0, 1.0);
            let t = 1.0 - radial * radial;
            let (mut r, mut g, mut b) = blend(bg_outer, bg_inner, t * 0.85);

            if sdf > -(ring_width * 2.2) {
                let rim_mix = ((ring_width * 2.2 + sdf) / (ring_width * 2.2)).clamp(0.0, 1.0);
                let rim_strength = 1.0 - rim_mix;
                let ring = blend((r, g, b), rim_color, rim_strength * 0.75);
                r = ring.0;
                g = ring.1;
                b = ring.2;
            }

            let glow = (1.0 - (distance / glow_radius)).clamp(0.0, 1.0);
            let glow_mix = glow * glow * 0.18;
            let glow_tinted = blend((r, g, b), (108, 152, 214), glow_mix);
            r = glow_tinted.0;
            g = glow_tinted.1;
            b = glow_tinted.2;

            let spark = spark_intensity(fx, fy, s);
            if spark > 0.0 {
                let spark_mix = spark.clamp(0.0, 1.0);
                let tinted = blend((r, g, b), spark_color, spark_mix * 0.88);
                r = tinted.0;
                g = tinted.1;
                b = tinted.2;
            }

            if distance < core_radius {
                let core_mix = (1.0 - distance / core_radius).clamp(0.0, 1.0);
                let core = blend((r, g, b), spark_core, core_mix * 0.95);
                r = core.0;
                g = core.1;
                b = core.2;
            }

            let alpha = if sdf > 0.0 {
                let fade = (outside_edge - sdf) / outside_edge;
                (fade.clamp(0.0, 1.0) * 255.0).round() as u8
            } else {
                255
            };

            let pixel = ((y * size + x) * 4) as usize;
            rgba[pixel] = r;
            rgba[pixel + 1] = g;
            rgba[pixel + 2] = b;
            rgba[pixel + 3] = alpha;
        }
    }

    rgba
}

fn spark_intensity(fx: f32, fy: f32, size: f32) -> f32 {
    let vertical = line_glow(fx, fy, 0.0, size * 0.040, size * 0.235);
    let horizontal = line_glow(fy, fx, 0.0, size * 0.040, size * 0.235);
    let diag_a = line_glow(fx - fy, fx + fy, 0.0, size * 0.050, size * 0.235);
    let diag_b = line_glow(fx + fy, fx - fy, 0.0, size * 0.050, size * 0.235);

    vertical.max(horizontal).max(diag_a).max(diag_b)
}

fn line_glow(
    axis_distance: f32,
    along_axis: f32,
    center: f32,
    half_width: f32,
    half_len: f32,
) -> f32 {
    if along_axis.abs() > half_len {
        return 0.0;
    }

    let dist = (axis_distance - center).abs();
    if dist >= half_width {
        return 0.0;
    }

    let width_falloff = 1.0 - dist / half_width;
    let length_falloff = 1.0 - (along_axis.abs() / half_len);
    width_falloff * width_falloff * length_falloff.sqrt()
}

fn blend(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let mix = t.clamp(0.0, 1.0);
    (
        lerp(a.0, b.0, mix),
        lerp(a.1, b.1, mix),
        lerp(a.2, b.2, mix),
    )
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

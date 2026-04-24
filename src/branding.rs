pub fn spark_icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let center = (s - 1.0) / 2.0;
    let half = s * 0.48;
    let corner_radius = s * 0.22;

    let ray_half_width = s * 0.055;
    let ray_length = s * 0.36;
    let diag_half_width = s * 0.045;
    let diag_length = s * 0.22;
    let core_radius = s * 0.085;

    let bg_fill = (22_u8, 25, 32);
    let bg_rim = (48_u8, 58, 76);
    let ray_color = (118_u8, 172, 240);
    let core_color = (178_u8, 214, 255);

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

            let (mut r, mut g, mut b) = if sdf > -1.4 { bg_rim } else { bg_fill };

            let axial_ray = (ax < ray_half_width && ay < ray_length)
                || (ay < ray_half_width && ax < ray_length);
            let diag_ray = ((fx - fy).abs() < diag_half_width
                && (ax + ay) < diag_length * 2.0
                && ax < diag_length
                && ay < diag_length)
                || ((fx + fy).abs() < diag_half_width
                    && (ax + ay) < diag_length * 2.0
                    && ax < diag_length
                    && ay < diag_length);

            if axial_ray || diag_ray {
                r = ray_color.0;
                g = ray_color.1;
                b = ray_color.2;
            }

            let distance = (fx * fx + fy * fy).sqrt();
            if distance < core_radius {
                r = core_color.0;
                g = core_color.1;
                b = core_color.2;
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

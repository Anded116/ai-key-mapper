use macroquad::models::{draw_mesh, Mesh, Vertex};
use macroquad::prelude::*;

const LIGHT: Vec3 = Vec3::new(0.42, 0.82, 0.38);

/// Flat directional light so boxes read as solids instead of silhouettes.
fn shade(c: Color, n: Vec3) -> Color {
    let k = 0.62 + 0.5 * n.dot(LIGHT.normalize()).max(0.0) + 0.06 * n.y;
    Color::new((c.r * k).min(1.0), (c.g * k).min(1.0), (c.b * k).min(1.0), c.a)
}

/// Unit box scaled to `size`, then transformed by `m`.
pub fn draw_box_m(m: &Mat4, size: Vec3, color: Color) {
    let h = size / 2.0;
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let mut n = Vec3::ZERO;
            n[axis] = sign;
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let c = shade(color, m.transform_vector3(n).normalize_or_zero());
            let base = vertices.len() as u16;
            for (su, sv) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut p = n * h;
                p[u] = su * h[u];
                p[v] = sv * h[v];
                vertices.push(Vertex::new2(m.transform_point3(p), Vec2::ZERO, c));
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    draw_mesh(&Mesh { vertices, indices, texture: None });
}

pub fn draw_box(center: Vec3, size: Vec3, color: Color) {
    draw_box_m(&Mat4::from_translation(center), size, color);
}

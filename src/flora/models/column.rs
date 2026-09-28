//! Game-owned stem geometry and assembly dimensions. Height changes rebuild
//! complete cells instead of stretching them; each cell is translated, not tilted.
use super::Triangle;
use glam::Vec3;

/// Uploaded with each flower part; shared by GPU deformation and CPU culling.
pub const MAX_BEND_FRACTION: f32 = 0.25;

#[derive(Clone, Copy, Debug)]
pub struct Column {
    pub edge: f32,
    pub height: f32,
    pub bend: f32,
    pub color: [u8; 3],
}
impl Column {
    pub fn for_flower(id: &str) -> anyhow::Result<Self> {
        let height = match id {
            "wild-geranium" | "corn-poppy" => 2.05,
            "forget-me-not" => 1.95,
            "oxeye-daisy" | "cosmos" | "bellflower" => 2.1,
            "coneflower" => 2.15,
            "tulip" => 1.85,
            _ => anyhow::bail!("unknown flower assembly: {id}"),
        };
        Ok(Self {
            edge: 0.05,
            height,
            bend: 0.08,
            color: [255; 3],
        })
    }
    pub fn count(self) -> u32 {
        // Published heights are exact multiples before f32 conversion.
        (self.height / self.edge - 1e-5).ceil().max(1.) as u32
    }
    pub fn center(self, layer: u32) -> Vec3 {
        let t = layer as f32 / self.count().saturating_sub(1).max(1) as f32;
        Vec3::new(
            self.bend * t * t * (3. - 2. * t),
            (layer as f32 + 0.5) * self.edge,
            0.,
        )
    }
    pub fn tip(self) -> Vec3 {
        Vec3::new(
            self.center(self.count() - 1).x,
            self.count() as f32 * self.edge,
            0.,
        )
    }
    pub fn scaled_height(self, scale: f32) -> Self {
        Self {
            height: self.height * scale,
            ..self
        }
    }
    pub fn triangles(self) -> Vec<Triangle> {
        let mut triangles = Vec::new();
        let half = self.edge * 0.5;
        for layer in 0..self.count() {
            let c = self.center(layer);
            // Complete cells keep their caps when wind exposes a formerly
            // internal face. Internal faces are hidden by the opaque volumes.
            for axis in [0, 2, 1] {
                let u = (axis + 1) % 3;
                let v = (axis + 2) % 3;
                for sign in [-1., 1.] {
                    face(
                        &mut triangles,
                        axis,
                        sign,
                        c[axis] + sign * half,
                        [c[u] - half, c[v] - half, c[u] + half, c[v] + half],
                        c,
                        self.color,
                    );
                }
            }
        }
        triangles
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::flora::models::{flowers, Shape};

    #[test]
    fn game_assembly_has_no_leaf_triangles() {
        for flower in flowers() {
            assert_eq!(flower.stem_triangles, flower.column.count() * 12);
            assert_eq!(flower.heads.len(), 1);
            assert_eq!(flower.heads[0].triangles.start, flower.stem_triangles);
            assert_eq!(flower.heads[0].anchor, flower.column.tip());
        }
    }

    #[test]
    fn native_flower_columns_keep_one_cube_per_layer_at_all_saved_heights() {
        for flower in flowers() {
            for height_scale in [0.25, 0.46, 1., 2., 4.] {
                let transformed = flower.transformed(Shape {
                    height_scale,
                    head_scale: 1.,
                });
                let column = transformed.column;
                assert_eq!(
                    transformed.stem_triangles,
                    column.count() * 12,
                    "six complete faces per moving cube"
                );
                let mut occupied = vec![false; column.count() as usize];
                for triangle in &transformed.triangles[..transformed.stem_triangles as usize] {
                    let layer = (triangle.anchor.y / column.edge - 0.5).round() as u32;
                    assert!(layer < column.count());
                    assert!(triangle.anchor.distance(column.center(layer)) < 2e-6);
                    occupied[layer as usize] = true;
                    for p in triangle.positions {
                        assert!(
                            (p - triangle.anchor).abs().max_element() <= column.edge * 0.5 + 2e-6
                        );
                    }
                }
                assert!(occupied.into_iter().all(|present| present));
                assert!(transformed.heads[0].anchor.distance(column.tip()) < 2e-6);
                // Shader deformation uses only XZ translation and smoothstep of
                // the rigid attachment height. Check even the square containing
                // the allowed circular displacement bound at every yaw.
                let limit = column.tip().y * MAX_BEND_FRACTION;
                for x in [-limit, 0., limit] {
                    for z in [-limit, 0., limit] {
                        let displacement = Vec3::new(x, 0., z);
                        let displaced = |p: Vec3| {
                            let t = (p.y / column.tip().y).clamp(0., 1.);
                            p + displacement * (t * t * (3. - 2. * t))
                        };
                        for layer in 1..column.count() {
                            let delta = displaced(column.center(layer))
                                - displaced(column.center(layer - 1));
                            assert!((delta.y - column.edge).abs() < 2e-6);
                            assert!(
                                delta.x.abs() < column.edge && delta.z.abs() < column.edge,
                                "{} at {height_scale}",
                                flower.id
                            );
                        }
                        let top = displaced(column.tip());
                        let cell = displaced(column.center(column.count() - 1));
                        assert!((top.x - cell.x).abs() < column.edge * 0.5);
                        assert!((top.z - cell.z).abs() < column.edge * 0.5);
                    }
                }
            }
        }
    }
}

fn face(
    out: &mut Vec<Triangle>,
    axis: usize,
    sign: f32,
    coordinate: f32,
    rect: [f32; 4],
    anchor: Vec3,
    color: [u8; 3],
) {
    let [lo_u, lo_v, hi_u, hi_v] = rect;
    if hi_u <= lo_u || hi_v <= lo_v {
        return;
    }
    let mut points = [[lo_u, lo_v], [hi_u, lo_v], [hi_u, hi_v], [lo_u, hi_v]].map(|[a, b]| {
        let mut p = Vec3::ZERO;
        p[axis] = coordinate;
        p[(axis + 1) % 3] = a;
        p[(axis + 2) % 3] = b;
        p
    });
    if sign < 0. {
        points.reverse();
    }
    let mut normal = Vec3::ZERO;
    normal[axis] = sign;
    for ids in [[0, 1, 2], [0, 2, 3]] {
        out.push(Triangle {
            positions: ids.map(|i| points[i]),
            normal,
            color,
            anchor,
        });
    }
}

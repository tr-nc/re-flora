//! Game-owned stem assembly dimensions. Stems render analytically; only heads
//! contribute triangles to the shared model surface bank.
use glam::Vec3;

/// Uploaded with each flower part; shared by GPU deformation and CPU culling.
pub const MAX_BEND_FRACTION: f32 = 0.25;

#[derive(Clone, Copy, Debug)]
pub struct Column {
    pub edge: f32,
    pub layers: u32,
    pub bend: f32,
}
impl Column {
    pub fn for_layers(layers: u32) -> anyhow::Result<Self> {
        anyhow::ensure!(
            (1..=512).contains(&layers),
            "invalid flower stem layers: {layers}"
        );
        Ok(Self {
            edge: 0.05,
            layers,
            bend: 0.08,
        })
    }
    pub fn count(self) -> u32 {
        self.layers
    }
    pub fn tip(self) -> Vec3 {
        Vec3::new(
            if self.layers > 1 { self.bend } else { 0. },
            self.layers as f32 * self.edge,
            0.,
        )
    }
    pub fn with_shape(self, shape: super::Shape) -> Self {
        let shape = shape.normalized();
        let edge = self.edge * shape.voxel_scale;
        let layers = shape.max_layers(self.layers);
        Self {
            edge,
            layers,
            bend: self
                .bend
                .min(edge * layers.saturating_sub(1) as f32 * MAX_BEND_FRACTION),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flora::models::{flowers, Shape};

    #[test]
    fn dimensions_and_height_distribution_remain_independent() {
        for flower in flowers() {
            let base = flower.column;
            for mean in [0.25, 0.46, 1., 4.] {
                for variance in [0., 0.01, 0.09, 1.] {
                    let shape = Shape {
                        height_scale: mean,
                        height_variance: variance,
                        ..Shape::default()
                    };
                    for z in [-8., -3., -1., 0., 1., 3., 8.] {
                        let layers = shape.layers_for_normal(base.layers, z);
                        assert!((1..=shape.max_layers(base.layers)).contains(&layers));
                        for size in [0.2, 0.9, 1., 4.] {
                            let scaled = Shape {
                                voxel_scale: size,
                                ..shape
                            };
                            assert_eq!(scaled.layers_for_normal(base.layers, z), layers);
                            let bank = base.with_shape(scaled);
                            assert_eq!(bank.edge, base.edge * size);
                            assert_eq!(bank.layers, shape.max_layers(base.layers));
                            assert_eq!(bank.tip().y, bank.layers as f32 * bank.edge);
                        }
                    }
                    if variance == 0. {
                        assert_eq!(
                            shape.layers_for_normal(base.layers, -8.),
                            shape.layers_for_normal(base.layers, 8.)
                        );
                    }
                }
            }
            assert_eq!(
                Shape {
                    height_variance: 0.09,
                    ..Shape::default()
                }
                .layers_for_normal(base.layers, 1.),
                (base.layers as f32 * 1.3).round() as u32
            );
        }
    }

    #[test]
    fn surface_bank_contains_only_heads_at_every_saved_height() {
        for flower in flowers() {
            for height_scale in [0.25, 0.46, 1., 2., 4.] {
                let transformed = flower.transformed(Shape {
                    height_scale,
                    ..Shape::default()
                });
                assert_eq!(transformed.heads.len(), 1);
                assert_eq!(transformed.heads[0].triangles.start, 0);
                assert_eq!(
                    transformed.heads[0].triangles.end as usize,
                    transformed.triangles.len()
                );
                assert_eq!(transformed.heads[0].anchor, transformed.column.tip());
            }
        }
    }
}

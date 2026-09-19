//! Geometry-derived relative response, not a calibrated material simulation.
//! Unit-density voxels supply modal mass; the rooted basal section supplies a
//! bending-stiffness proxy. Canonical (non-LOD) geometry is the only authority.
use crate::tracer::voxel_encoding::FloraVoxelInfoEntry;

pub(crate) fn frequency_proxy(entries: &[FloraVoxelInfoEntry]) -> f32 {
    if entries.is_empty() {
        return 1.;
    }
    // Model construction may contain overlapping stem/flower voxels: count mass once.
    let points: std::collections::BTreeSet<_> = entries
        .iter()
        .map(|e| (e.pos.x, e.pos.y, e.pos.z))
        .collect();
    let base = points.iter().map(|p| p.1).min().unwrap();
    let height = (points.iter().map(|p| p.1).max().unwrap() - base).max(1) as f32;
    let mass: f32 = points
        .iter()
        .map(|p| {
            let s = (p.1 - base) as f32 / height;
            let mode = s * s * (3. - s) * 0.5;
            mode * mode
        })
        .sum();
    let basal: Vec<_> = points.iter().filter(|p| p.1 == base).collect();
    let n = basal.len() as f32;
    let cx = basal.iter().map(|p| p.0 as f32).sum::<f32>() / n;
    let cz = basal.iter().map(|p| p.2 as f32).sum::<f32>() / n;
    let inertia: f32 = basal
        .iter()
        .map(|p| {
            // Polar average of square-voxel section moments, including own inertia.
            ((p.0 as f32 - cx).powi(2) + (p.2 as f32 - cz).powi(2)) * 0.5 + 1. / 12.
        })
        .sum();
    (3. * inertia / (height.powi(3) * mass.max(0.01))).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracer::voxel_encoding::FloraVoxelInfo;
    fn point(x: i32, y: i32) -> FloraVoxelInfoEntry {
        FloraVoxelInfoEntry {
            pos: glam::IVec3::new(x, y, 0),
            info: FloraVoxelInfo::fallback(),
        }
    }
    #[test]
    fn taller_and_top_heavier_models_respond_more_slowly() {
        let short: Vec<_> = (0..4).map(|y| point(0, y)).collect();
        let tall: Vec<_> = (0..8).map(|y| point(0, y)).collect();
        assert!(frequency_proxy(&tall) < frequency_proxy(&short));
        let mut heavy = tall.clone();
        heavy.extend([-2, -1, 1, 2].map(|x| point(x, 7)));
        assert!(frequency_proxy(&heavy) < frequency_proxy(&tall));
        let mut duplicate = tall.clone();
        duplicate.extend(tall.clone());
        assert_eq!(frequency_proxy(&duplicate), frequency_proxy(&tall));
    }
}

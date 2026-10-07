//! Published static snapshots of the shared web recipe. No soil simulation/accounting.
use glam::{Quat, Vec3};
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
pub(crate) struct Part {
    pub material: String,
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub uvs: Vec<f32>,
    pub indices: Vec<u32>,
}
#[derive(Deserialize)]
pub(crate) struct State {
    pub id: String,
    pub angle: f32,
    pub mouth: [f32; 3],
    pub parts: Vec<Part>,
}
#[derive(Deserialize)]
pub(crate) struct Asset {
    pub colors: [[f32; 3]; 4],
    pub texture_size: [u32; 2],
    pub states: [State; 2],
}
pub(crate) fn asset() -> &'static Asset {
    static ASSET: LazyLock<Asset> = LazyLock::new(|| {
        let asset: Asset = serde_json::from_str(include_str!("../assets/models/soil-bag.json"))
            .expect("validated published soil-bag asset");
        assert_eq!(asset.states[0].id, "hover");
        assert_eq!(asset.states[1].id, "pour");
        asset
    });
    &ASSET
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Preview {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    pub pouring: bool,
}
impl Preview {
    pub fn over_surface(target: Vec3, camera: Vec3, pouring: bool) -> Self {
        let state = &asset().states[usize::from(pouring)];
        let to_camera = camera - target;
        let yaw = to_camera.x.atan2(to_camera.z);
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(state.angle);
        let scale = 0.10;
        // Keep the actual mouth above the cursor while pouring. The common view
        // quantizer pivots on this mouth, so the rendered lip and emitter agree.
        let position = if pouring {
            target + Vec3::Y * 0.14 - rotation * Vec3::from(state.mouth) * scale
        } else {
            target + Vec3::Y * 0.15
        };
        Self {
            position,
            rotation,
            scale,
            pouring,
        }
    }
    pub fn mouth(&self) -> Vec3 {
        self.position
            + self.rotation
                * Vec3::from(asset().states[usize::from(self.pouring)].mouth)
                * self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn soil_bag_published_geometry_has_valid_normals_uvs_and_one_static_downward_pose() {
        let asset = asset();
        assert_eq!(asset.texture_size, [512, 640]);
        assert_eq!(asset.states[0].id, "hover");
        assert_eq!(asset.states[1].id, "pour");
        assert_eq!(asset.states[0].angle, 0.);
        assert!(asset.states[1].angle.cos() < -0.7);
        for state in &asset.states {
            assert_eq!(state.parts.len(), 6);
            for part in &state.parts {
                assert_eq!(part.positions.len(), part.normals.len());
                assert_eq!(part.uvs.len(), part.positions.len() / 3 * 2);
                assert!(part
                    .positions
                    .iter()
                    .chain(&part.normals)
                    .all(|v| v.is_finite()));
                assert!(part
                    .indices
                    .iter()
                    .all(|&i| (i as usize) < part.positions.len() / 3));
            }
        }
    }
    #[test]
    fn soil_bag_render_adapter_uses_shared_sampling_and_vulkan_vertex_indices() {
        let shader = include_str!("../shader/slang/tool_model.vert.slang");
        assert!(shader.contains("cachedModelVertex("));
        assert!(shader.contains("modelMeshViewFrame("));
        assert!(shader.contains("SV_VulkanVertexID"));
        assert!(!shader.contains(":SV_VertexID"));
        assert!(!shader.contains("SV_Depth"));
        assert!(shader.contains("pose.mouth.xyz,cached"));
    }

    #[test]
    fn soil_bag_preview_keeps_pour_mouth_on_target_without_animation_or_view_dependent_drift() {
        let target = Vec3::new(1., 0.6, 2.);
        for camera in [
            Vec3::new(1., 1., 3.),
            Vec3::new(2., 1., 2.),
            Vec3::new(0., 2., 1.),
        ] {
            let hover = Preview::over_surface(target, camera, false);
            let pour = Preview::over_surface(target, camera, true);
            assert!((pour.mouth() - (target + Vec3::Y * 0.14)).length() < 1e-6);
            assert!((pour.rotation * Vec3::Y).y < -0.7);
            assert!((hover.rotation * Vec3::Y).y > 0.999);
        }
    }
}

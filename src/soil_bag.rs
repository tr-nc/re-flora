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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Instance {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    pub pouring: bool,
}
impl Instance {
    pub fn over_surface(target: Vec3, camera: Vec3, pouring: bool) -> Self {
        let state = &asset().states[usize::from(pouring)];
        let to_camera = camera - target;
        let yaw = to_camera.x.atan2(to_camera.z);
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(state.angle);
        let scale = 0.05;
        // Keep the actual mouth above the cursor while pouring. The common view
        // quantizer pivots on this mouth, so the rendered lip and emitter agree.
        let position = if pouring {
            target + Vec3::Y * (scale * 1.4) - rotation * Vec3::from(state.mouth) * scale
        } else {
            target + Vec3::Y * (scale * 1.5)
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

/// Direct-sun history follows the actually rendered instance. Camera/view-bank
/// changes also matter because shared pixel models quantize world orientation.
#[derive(Default)]
pub(crate) struct ShadowState {
    previous: Option<(Instance, Vec3, u32)>,
}
impl ShadowState {
    pub fn observe(&mut self, instance: Option<Instance>, camera: Vec3, views: u32) -> bool {
        let current = instance.map(|instance| (instance, camera, views));
        let changed = self.previous != current;
        self.previous = current;
        changed
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
        let placement = include_str!("../shader/slang/world_model_instance.slang");
        let shadow = include_str!("../shader/slang/tool_model_shadow.vert.slang");
        assert!(placement.contains("modelMeshViewFrame("));
        assert!(shader.contains("world_model_instances[instance]"));
        assert!(shadow.contains("world_model_instances[instance]"));
        assert!(shadow.contains("worldModelRenderedFrame(pose)"));
        assert!(shadow.contains("shadow_camera_info.view_proj_mat"));
        assert!(shadow.contains("import model_mesh_geometry;"));
        assert!(!shadow.contains("import model_mesh;"));
        assert!(!shadow.contains("import model_surface_cache;"));
        assert!(!shadow.contains("cachedModelVertex("));
        assert!(shader.contains("SV_VulkanInstanceID"));
        assert!(shadow.contains("SV_VulkanInstanceID"));
        assert!(!shadow.contains(":SV_VertexID"));
        assert!(shader.contains("meshShade("));
        assert!(shader.contains("SV_VulkanVertexID"));
        assert!(!shader.contains(":SV_VertexID"));
        assert!(!shader.contains("SV_Depth"));
        assert!(shader.contains("pose.mouth.xyz,cached"));
    }

    #[test]
    fn world_bag_shadow_history_tracks_creation_pose_camera_views_and_removal() {
        let camera = Vec3::new(0., 1., 2.);
        let hover = Instance::over_surface(Vec3::ZERO, camera, false);
        let pour = Instance::over_surface(Vec3::ZERO, camera, true);
        let mut state = ShadowState::default();
        assert!(!state.observe(None, camera, 128));
        assert!(state.observe(Some(hover), camera, 128));
        assert!(!state.observe(Some(hover), camera, 128));
        assert!(state.observe(Some(pour), camera, 128));
        assert!(state.observe(Some(pour), camera + Vec3::Y, 128));
        assert!(state.observe(Some(pour), camera + Vec3::Y, 32));
        let moved = Instance {
            position: pour.position + Vec3::X,
            ..pour
        };
        assert!(state.observe(Some(moved), camera + Vec3::Y, 32));
        assert!(state.observe(None, camera, 32));
        assert!(!state.observe(None, camera + Vec3::Z, 256));
    }

    #[test]
    fn world_bag_participates_in_scene_passes_even_when_flora_and_particles_are_disabled() {
        let renderer = include_str!("tracer/mod.rs");
        assert_eq!(
            renderer
                .matches("|| self.model_mesh_frame.has_soil_bag()")
                .count(),
            2
        );
        assert!(renderer.contains("record_soil_bag_shadow_pass"));
    }

    #[test]
    fn soil_bag_world_instance_uses_half_sized_body_and_proportional_surface_clearance() {
        let target = Vec3::new(0.4, 0.8, 1.2);
        let camera = Vec3::new(0.4, 1.2, 2.2);
        let hover = Instance::over_surface(target, camera, false);
        let pour = Instance::over_surface(target, camera, true);
        assert_eq!(
            hover.scale, 0.05,
            "bag must be half its former 0.10 world scale"
        );
        assert_eq!(pour.scale, hover.scale);
        assert!((hover.position.y - target.y - 0.075).abs() < 1e-6);
        assert!((pour.mouth() - target - Vec3::Y * 0.07).length() < 1e-6);
    }

    #[test]
    fn soil_bag_instance_keeps_pour_mouth_on_target_without_animation_or_view_dependent_drift() {
        let target = Vec3::new(1., 0.6, 2.);
        for camera in [
            Vec3::new(1., 1., 3.),
            Vec3::new(2., 1., 2.),
            Vec3::new(0., 2., 1.),
        ] {
            let hover = Instance::over_surface(target, camera, false);
            let pour = Instance::over_surface(target, camera, true);
            assert!((pour.mouth() - (target + Vec3::Y * (pour.scale * 1.4))).length() < 1e-6);
            assert!((pour.rotation * Vec3::Y).y < -0.7);
            assert!((hover.rotation * Vec3::Y).y > 0.999);
        }
    }
}

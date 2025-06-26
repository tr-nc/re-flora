use glam::{Mat4, Vec3};

use crate::gameplay::Camera; // Assuming your player camera is in `gameplay`

/// Defines the properties of the light source for shadow calculation.
pub enum LightType {
    /// For lights like the sun, with a direction but no position.
    Directional { direction: Vec3, up: Vec3 },
    /// For lights with a position and a limited cone, like a flashlight.
    Spot {
        position: Vec3,
        direction: Vec3,
        up: Vec3,
        /// The full angle of the light cone in radians.
        fov_y: f32,
    },
}

pub struct ShadowCamera {
    view_matrix: Mat4,
    projection_matrix: Mat4,
}

impl ShadowCamera {
    pub fn new() -> Self {
        Self {
            view_matrix: Mat4::IDENTITY,
            projection_matrix: Mat4::IDENTITY,
        }
    }

    /// Updates the shadow camera's matrices based on the player's view and the light source.
    ///
    /// - `player_camera`: The main camera the user is looking through.
    /// - `light`: The properties of the light casting the shadow.
    /// - `shadow_map_resolution`: The width/height of the shadow map texture. Needed for stabilization.
    pub fn update(
        &mut self,
        player_camera: &Camera,
        light: &LightType,
        shadow_map_resolution: [u32; 2],
    ) {
        match light {
            LightType::Directional { direction, up } => {
                self.update_directional(player_camera, *direction, *up, shadow_map_resolution);
            }
            LightType::Spot {
                position,
                direction,
                up,
                fov_y,
            } => {
                self.update_spot(player_camera, *position, *direction, *up, *fov_y);
            }
        }
    }

    /// Calculates the view-projection matrix for a directional light.
    fn update_directional(
        &mut self,
        player_camera: &Camera,
        light_direction: Vec3,
        light_up: Vec3,
        shadow_map_resolution: [u32; 2],
    ) {
        // 1. Get the 8 corners of the player's frustum in world space.
        let frustum_corners = player_camera.get_frustum_corners();

        // 2. Center the light's "look at" point on the player's frustum.
        // This makes the shadow map follow the player.
        let mut frustum_center = Vec3::ZERO;
        for corner in &frustum_corners {
            frustum_center += *corner;
        }
        frustum_center /= 8.0;

        // 3. Create the light's view matrix.
        // We position the "eye" of the light camera far away from the center,
        // looking back at it along the light's direction.
        let light_eye_pos = frustum_center - light_direction * player_camera.get_far_plane() * 2.0;
        self.view_matrix = Mat4::look_at_rh(light_eye_pos, frustum_center, light_up);

        // 4. Transform the frustum corners to light space.
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for corner in &frustum_corners {
            let trf = self.view_matrix * corner.extend(1.0);
            min = min.min(trf.truncate());
            max = max.max(trf.truncate());
        }

        // 5. STABILIZATION: Align the projection to the shadow map's texel grid.
        // This prevents the shimmering artifact as the camera moves.
        let world_units_per_texel = Vec3::new(max.x - min.x, max.y - min.y, max.z - min.z)
            / shadow_map_resolution[0] as f32;

        min /= world_units_per_texel;
        min = min.floor();
        min *= world_units_per_texel;

        max /= world_units_per_texel;
        max = max.floor();
        max *= world_units_per_texel;

        // 6. Create the tight-fitting orthographic projection.
        self.projection_matrix = Mat4::orthographic_rh(min.x, max.x, min.y, max.y, min.z, max.z);
    }

    /// Calculates the view-projection matrix for a spot light.
    fn update_spot(
        &mut self,
        player_camera: &Camera,
        light_position: Vec3,
        light_direction: Vec3,
        light_up: Vec3,
        light_fov_y: f32,
    ) {
        // View matrix is simple: look from the light's position in its direction.
        self.view_matrix =
            Mat4::look_at_rh(light_position, light_position + light_direction, light_up);

        // Projection is a standard perspective frustum matching the light's cone.
        // Aspect ratio is 1.0 because shadow maps are usually square.
        // Near/far planes should match the light's effective range.
        self.projection_matrix =
            Mat4::perspective_rh(light_fov_y, 1.0, 0.1, player_camera.get_far_plane());
    }

    pub fn get_view_mat(&self) -> Mat4 {
        self.view_matrix
    }

    pub fn get_proj_mat(&self) -> Mat4 {
        self.projection_matrix
    }

    pub fn get_view_proj_mat(&self) -> Mat4 {
        self.projection_matrix * self.view_matrix
    }
}

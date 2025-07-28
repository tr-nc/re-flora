use crate::tracer::TracerResources;
use crate::vkn::{Buffer, PlainMemberTypeWithData, StructMemberDataBuilder};
use anyhow::Result;
use glam::{Mat4, Vec3};

pub struct BufferUpdater;

impl BufferUpdater {
    pub fn update_camera_info(
        camera_info: &mut Buffer,
        view_mat: Mat4,
        proj_mat: Mat4,
    ) -> Result<()> {
        let view_proj_mat = proj_mat * view_mat;

        let camera_pos = view_mat.inverse().w_axis;
        let data = StructMemberDataBuilder::from_buffer(camera_info)
            .set_field("pos", PlainMemberTypeWithData::Vec4(camera_pos.to_array()))
            .set_field(
                "view_mat",
                PlainMemberTypeWithData::Mat4(view_mat.to_cols_array_2d()),
            )
            .set_field(
                "view_mat_inv",
                PlainMemberTypeWithData::Mat4(view_mat.inverse().to_cols_array_2d()),
            )
            .set_field(
                "proj_mat",
                PlainMemberTypeWithData::Mat4(proj_mat.to_cols_array_2d()),
            )
            .set_field(
                "proj_mat_inv",
                PlainMemberTypeWithData::Mat4(proj_mat.inverse().to_cols_array_2d()),
            )
            .set_field(
                "view_proj_mat",
                PlainMemberTypeWithData::Mat4(view_proj_mat.to_cols_array_2d()),
            )
            .set_field(
                "view_proj_mat_inv",
                PlainMemberTypeWithData::Mat4(view_proj_mat.inverse().to_cols_array_2d()),
            )
            .build()?;
        camera_info.fill_with_raw_u8(&data)?;
        Ok(())
    }

    pub fn update_denoiser_info(
        temporal_info: &mut Buffer,
        spatial_info: &mut Buffer,
        temporal_position_phi: f32,
        temporal_alpha: f32,
        phi_c: f32,
        phi_n: f32,
        phi_p: f32,
        min_phi_z: f32,
        max_phi_z: f32,
        phi_z_stable_sample_count: f32,
        is_changing_lum_phi: bool,
        is_spatial_denoising_enabled: bool,
    ) -> Result<()> {
        Self::update_temporal_info(temporal_info, temporal_position_phi, temporal_alpha)?;
        Self::update_spatial_info(
            spatial_info,
            phi_c,
            phi_n,
            phi_p,
            min_phi_z,
            max_phi_z,
            phi_z_stable_sample_count,
            is_changing_lum_phi,
            is_spatial_denoising_enabled,
        )?;
        Ok(())
    }

    fn update_temporal_info(
        temporal_info: &mut Buffer,
        temporal_position_phi: f32,
        temporal_alpha: f32,
    ) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(temporal_info)
            .set_field(
                "temporal_position_phi",
                PlainMemberTypeWithData::Float(temporal_position_phi),
            )
            .set_field(
                "temporal_alpha",
                PlainMemberTypeWithData::Float(temporal_alpha),
            )
            .build()?;
        temporal_info.fill_with_raw_u8(&data)?;
        Ok(())
    }

    fn update_spatial_info(
        spatial_info: &mut Buffer,
        phi_c: f32,
        phi_n: f32,
        phi_p: f32,
        min_phi_z: f32,
        max_phi_z: f32,
        phi_z_stable_sample_count: f32,
        is_changing_lum_phi: bool,
        is_spatial_denoising_enabled: bool,
    ) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(spatial_info)
            .set_field("phi_c", PlainMemberTypeWithData::Float(phi_c))
            .set_field("phi_n", PlainMemberTypeWithData::Float(phi_n))
            .set_field("phi_p", PlainMemberTypeWithData::Float(phi_p))
            .set_field("min_phi_z", PlainMemberTypeWithData::Float(min_phi_z))
            .set_field("max_phi_z", PlainMemberTypeWithData::Float(max_phi_z))
            .set_field(
                "phi_z_stable_sample_count",
                PlainMemberTypeWithData::Float(phi_z_stable_sample_count),
            )
            .set_field(
                "is_changing_lum_phi",
                PlainMemberTypeWithData::UInt(is_changing_lum_phi as u32),
            )
            .set_field(
                "is_spatial_denoising_enabled",
                PlainMemberTypeWithData::UInt(is_spatial_denoising_enabled as u32),
            )
            .build()?;
        spatial_info.fill_with_raw_u8(&data)?;
        Ok(())
    }

    pub fn update_gui_input(
        resources: &TracerResources,
        debug_float: f32,
        debug_bool: bool,
        debug_uint: u32,
    ) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(&resources.gui_input)
            .set_field("debug_float", PlainMemberTypeWithData::Float(debug_float))
            .set_field(
                "debug_bool",
                PlainMemberTypeWithData::UInt(debug_bool as u32),
            )
            .set_field("debug_uint", PlainMemberTypeWithData::UInt(debug_uint))
            .build()?;
        resources.gui_input.fill_with_raw_u8(&data)?;
        Ok(())
    }

    pub fn update_sun_info(
        resources: &TracerResources,
        sun_dir: Vec3,
        sun_size: f32,
        sun_color: Vec3,
        sun_luminance: f32,
        sun_altitude: f32,
        sun_azimuth: f32,
    ) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(&resources.sun_info)
            .set_field("sun_dir", PlainMemberTypeWithData::Vec3(sun_dir.to_array()))
            .set_field("sun_size", PlainMemberTypeWithData::Float(sun_size))
            .set_field(
                "sun_color",
                PlainMemberTypeWithData::Vec3(sun_color.to_array()),
            )
            .set_field(
                "sun_luminance",
                PlainMemberTypeWithData::Float(sun_luminance),
            )
            .set_field("sun_altitude", PlainMemberTypeWithData::Float(sun_altitude))
            .set_field("sun_azimuth", PlainMemberTypeWithData::Float(sun_azimuth))
            .build()?;
        resources.sun_info.fill_with_raw_u8(&data)?;
        Ok(())
    }

    pub fn update_shading_info(resources: &TracerResources, ambient_light: Vec3) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(&resources.shading_info)
            .set_field(
                "ambient_light",
                PlainMemberTypeWithData::Vec3(ambient_light.to_array()),
            )
            .build()?;
        resources.shading_info.fill_with_raw_u8(&data)?;
        Ok(())
    }

    pub fn update_starlight_info(
        resources: &TracerResources,
        iterations: i32,
        formuparam: f32,
        volsteps: i32,
        stepsize: f32,
        zoom: f32,
        tile: f32,
        speed: f32,
        brightness: f32,
        darkmatter: f32,
        distfading: f32,
        saturation: f32,
    ) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(&resources.starlight_info)
            .set_field("iterations", PlainMemberTypeWithData::Int(iterations))
            .set_field("formuparam", PlainMemberTypeWithData::Float(formuparam))
            .set_field("volsteps", PlainMemberTypeWithData::Int(volsteps))
            .set_field("stepsize", PlainMemberTypeWithData::Float(stepsize))
            .set_field("zoom", PlainMemberTypeWithData::Float(zoom))
            .set_field("tile", PlainMemberTypeWithData::Float(tile))
            .set_field("speed", PlainMemberTypeWithData::Float(speed))
            .set_field("brightness", PlainMemberTypeWithData::Float(brightness))
            .set_field("darkmatter", PlainMemberTypeWithData::Float(darkmatter))
            .set_field("distfading", PlainMemberTypeWithData::Float(distfading))
            .set_field("saturation", PlainMemberTypeWithData::Float(saturation))
            .build()?;
        resources.starlight_info.fill_with_raw_u8(&data)?;
        Ok(())
    }

    pub fn update_env_info(resources: &TracerResources, frame_serial_idx: u32) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(&resources.env_info)
            .set_field(
                "frame_serial_idx",
                PlainMemberTypeWithData::UInt(frame_serial_idx),
            )
            .build()?;
        resources.env_info.fill_with_raw_u8(&data)?;
        Ok(())
    }

    pub fn update_voxel_colors(
        resources: &TracerResources,
        sand_color: Vec3,
        dirt_color: Vec3,
        rock_color: Vec3,
        leaf_color: Vec3,
        trunk_color: Vec3,
    ) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(&resources.voxel_colors)
            .set_field(
                "sand_color",
                PlainMemberTypeWithData::Vec3(sand_color.to_array()),
            )
            .set_field(
                "dirt_color",
                PlainMemberTypeWithData::Vec3(dirt_color.to_array()),
            )
            .set_field(
                "rock_color",
                PlainMemberTypeWithData::Vec3(rock_color.to_array()),
            )
            .set_field(
                "leaf_color",
                PlainMemberTypeWithData::Vec3(leaf_color.to_array()),
            )
            .set_field(
                "trunk_color",
                PlainMemberTypeWithData::Vec3(trunk_color.to_array()),
            )
            .build()?;
        resources.voxel_colors.fill_with_raw_u8(&data)?;
        Ok(())
    }

    pub fn update_taa_info(resources: &TracerResources, is_taa_enabled: bool) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(&resources.taa_info)
            .set_field(
                "is_taa_enabled",
                PlainMemberTypeWithData::UInt(is_taa_enabled as u32),
            )
            .build()?;
        resources.taa_info.fill_with_raw_u8(&data)?;
        Ok(())
    }

    pub fn update_god_ray_info(
        resources: &TracerResources,
        max_depth: f32,
        max_checks: u32,
        weight: f32,
        color: Vec3,
    ) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(&resources.god_ray_info)
            .set_field("max_depth", PlainMemberTypeWithData::Float(max_depth))
            .set_field("max_checks", PlainMemberTypeWithData::UInt(max_checks))
            .set_field("weight", PlainMemberTypeWithData::Float(weight))
            .set_field("color", PlainMemberTypeWithData::Vec3(color.to_array()))
            .build()?;
        resources.god_ray_info.fill_with_raw_u8(&data)?;
        Ok(())
    }

    pub fn update_post_processing_info(
        resources: &TracerResources,
        scaling_factor: f32,
    ) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(&resources.post_processing_info)
            .set_field(
                "scaling_factor",
                PlainMemberTypeWithData::Float(scaling_factor),
            )
            .build()?;
        resources.post_processing_info.fill_with_raw_u8(&data)?;
        Ok(())
    }

    pub fn update_player_collider_info(
        resources: &TracerResources,
        player_pos: Vec3,
        camera_front: Vec3,
    ) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(&resources.player_collider_info)
            .set_field(
                "player_pos",
                PlainMemberTypeWithData::Vec3(player_pos.to_array()),
            )
            .set_field(
                "camera_front",
                PlainMemberTypeWithData::Vec3(camera_front.to_array()),
            )
            .build()?;
        resources.player_collider_info.fill_with_raw_u8(&data)?;
        Ok(())
    }
}

use ash::vk;
use std::sync::Arc;

use crate::vkn::Device;

#[derive(Copy, Clone)]
pub struct SamplerDesc {
    pub mag_filter: vk::Filter,
    pub min_filter: vk::Filter,
    pub address_mode_u: vk::SamplerAddressMode,
    pub address_mode_v: vk::SamplerAddressMode,
    pub address_mode_w: vk::SamplerAddressMode,
    pub anisotropy_enable: bool,
    pub max_anisotropy: f32,
    pub border_color: vk::BorderColor,
    pub unnormalized_coordinates: bool,
    pub compare_enable: bool,
    pub compare_op: vk::CompareOp,
    pub mipmap_mode: vk::SamplerMipmapMode,
    pub mip_lod_bias: f32,
    pub min_lod: f32,
    pub max_lod: f32,
}

impl Default for SamplerDesc {
    fn default() -> Self {
        Self {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            address_mode_u: vk::SamplerAddressMode::CLAMP_TO_EDGE,
            address_mode_v: vk::SamplerAddressMode::CLAMP_TO_EDGE,
            address_mode_w: vk::SamplerAddressMode::CLAMP_TO_EDGE,
            anisotropy_enable: false,
            max_anisotropy: 1.0,
            border_color: vk::BorderColor::INT_OPAQUE_BLACK,
            unnormalized_coordinates: false,
            compare_enable: false,
            compare_op: vk::CompareOp::ALWAYS,
            mipmap_mode: vk::SamplerMipmapMode::LINEAR,
            mip_lod_bias: 0.0,
            min_lod: 0.0,
            max_lod: 1.0,
        }
    }
}

struct SamplerInner {
    device: Device,
    sampler: vk::Sampler,
}

impl Drop for SamplerInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_sampler(self.sampler, None);
        }
    }
}

#[derive(Clone)]
pub struct Sampler(Arc<SamplerInner>);

impl std::ops::Deref for Sampler {
    type Target = vk::Sampler;
    fn deref(&self) -> &Self::Target {
        &self.0.sampler
    }
}

impl Sampler {
    pub fn new(device: &Device, desc: SamplerDesc) -> Self {
        let sampler = create_sampler(device.as_raw(), &desc);
        Self(Arc::new(SamplerInner {
            device: device.clone(),
            sampler,
        }))
    }

    pub fn as_raw(&self) -> vk::Sampler {
        self.0.sampler
    }
}

fn create_sampler(device: &ash::Device, desc: &SamplerDesc) -> vk::Sampler {
    let sampler = {
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(desc.mag_filter)
            .min_filter(desc.min_filter)
            .address_mode_u(desc.address_mode_u)
            .address_mode_v(desc.address_mode_v)
            .address_mode_w(desc.address_mode_w)
            .anisotropy_enable(desc.anisotropy_enable)
            .max_anisotropy(desc.max_anisotropy)
            .border_color(desc.border_color)
            .unnormalized_coordinates(desc.unnormalized_coordinates)
            .compare_enable(desc.compare_enable)
            .compare_op(desc.compare_op)
            .mipmap_mode(desc.mipmap_mode)
            .mip_lod_bias(desc.mip_lod_bias)
            .min_lod(desc.min_lod)
            .max_lod(desc.max_lod);
        unsafe { device.create_sampler(&sampler_info, None).unwrap() }
    };
    sampler
}

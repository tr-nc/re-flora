use ash::vk;
use std::sync::Arc;

use crate::vkn::Device;

use super::SamplerDesc;

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
    pub fn new(device: Device, desc: &SamplerDesc) -> Self {
        let sampler = create_sampler(device.as_raw(), &desc);
        Self(Arc::new(SamplerInner { device, sampler }))
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

//! Opt-in readback at the production DDGI consumer seam. No normal-frame dispatch/readback.
use crate::{
    ddgi::DdgiFieldIdentity,
    generated::gpu_structs::{DdgiResponseRequest, DdgiResponseSample},
    resource::Resource,
};
use anyhow::{ensure, Result};
use bytemuck::Zeroable;
use glam::Vec3;
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, ComputePipeline, Device,
    Extent3D, MemoryLocation,
};
use resource_container_derive::ResourceContainer;

#[derive(ResourceContainer)]
pub(super) struct DdgiResponseResources {
    pub ddgi_response_request: Resource<Buffer>,
    pub ddgi_response_sample: Resource<Buffer>,
    readback: Buffer,
}

impl DdgiResponseResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let request = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::UNIFORM_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of::<DdgiResponseRequest>() as u64,
        );
        request
            .fill_uniform(&DdgiResponseRequest::zeroed())
            .unwrap();
        let sample = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
            ),
            MemoryLocation::GpuOnly,
            std::mem::size_of::<DdgiResponseSample>() as u64,
        );
        let readback = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            std::mem::size_of::<DdgiResponseSample>() as u64,
        );
        Self {
            ddgi_response_request: Resource::new(request),
            ddgi_response_sample: Resource::new(sample),
            readback,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DdgiResponseEvidence {
    pub serial: u32,
    pub ready: bool,
    pub geometry_revision: u32,
    pub radiance_revision: u32,
    pub consumer_geometry_revision: u32,
    pub consumer_radiance_revision: u32,
    pub field_serial: u64,
    pub irradiance: Vec3,
    pub weight: f32,
    pub probes: u32,
}

#[derive(Default)]
pub(super) struct DdgiResponseSampler {
    next_serial: u32,
    queued: Option<DdgiResponseRequest>,
    in_flight: Option<(DdgiResponseRequest, Option<DdgiFieldIdentity>)>,
    published: Option<DdgiResponseEvidence>,
}

impl DdgiResponseSampler {
    pub fn request(
        &mut self,
        resources: &DdgiResponseResources,
        position: Vec3,
        normal: Vec3,
    ) -> Result<u32> {
        ensure!(
            self.queued.is_none() && self.in_flight.is_none(),
            "DDGI response sample still pending"
        );
        ensure!(
            position.is_finite() && normal.is_finite() && normal.length_squared() > 0.0,
            "invalid DDGI response receiver"
        );
        self.next_serial = self
            .next_serial
            .checked_add(1)
            .expect("DDGI response serial overflow");
        let request = DdgiResponseRequest {
            identity: [self.next_serial, 0, 0, 0],
            position: position.extend(1.0).to_array(),
            normal: normal.normalize().extend(0.0).to_array(),
        };
        resources.ddgi_response_request.fill_uniform(&request)?;
        self.queued = Some(request);
        Ok(self.next_serial)
    }

    pub fn record(
        &mut self,
        resources: &DdgiResponseResources,
        pipeline: &ComputePipeline,
        cmdbuf: &CommandBuffer,
        field: Option<DdgiFieldIdentity>,
    ) {
        let Some(request) = self.queued.take() else {
            return;
        };
        assert!(self.in_flight.is_none());
        pipeline.record(cmdbuf, Extent3D::new(1, 1, 1), None);
        resources.ddgi_response_sample.record_copy_to_buffer(
            cmdbuf,
            &resources.readback,
            std::mem::size_of::<DdgiResponseSample>() as u64,
            0,
            0,
        );
        cmdbuf.use_buffer(&resources.readback, BufferUse::HostRead);
        self.in_flight = Some((request, field));
    }

    pub fn resolve(&mut self, resources: &DdgiResponseResources) -> Result<()> {
        let Some((request, field)) = self.in_flight.take() else {
            return Ok(());
        };
        let bytes = resources.readback.read_back()?;
        ensure!(
            bytes.len() == std::mem::size_of::<DdgiResponseSample>(),
            "DDGI response readback size mismatch"
        );
        let value: DdgiResponseSample = bytemuck::pod_read_unaligned(&bytes);
        ensure!(
            value.identity[0] == request.identity[0]
                && value.position == request.position
                && value.normal == request.normal,
            "DDGI response readback lost its receiver/request identity"
        );
        ensure!(
            value.identity[1] <= 1
                && value
                    .irradiance_and_weight
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0.0),
            "invalid DDGI response value"
        );
        self.published = Some(DdgiResponseEvidence {
            serial: value.identity[0],
            ready: value.identity[1] == 1,
            geometry_revision: field.map_or(0, |f| f.field().geometry_revision()),
            radiance_revision: field.map_or(0, |f| f.field().radiance_revision()),
            consumer_geometry_revision: value.identity[2],
            consumer_radiance_revision: value.identity[3],
            field_serial: field.map_or(0, |f| f.field().serial()),
            irradiance: Vec3::from_slice(&value.irradiance_and_weight),
            weight: value.irradiance_and_weight[3],
            probes: value.support[0],
        });
        Ok(())
    }

    pub fn published(&self) -> Option<DdgiResponseEvidence> {
        self.published
    }
}

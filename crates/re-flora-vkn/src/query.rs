use crate::{CommandBuffer, Device, PipelineStage, VulkanContext};
use ash::{prelude::VkResult, vk};

pub struct TimestampQueryPool {
    device: Device,
    query_pool: vk::QueryPool,
    timestamp_period_ns: f32,
    max_query_count: u32,
}

impl Drop for TimestampQueryPool {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_query_pool(self.query_pool, None);
        }
    }
}

impl TimestampQueryPool {
    pub fn maybe_new(
        vulkan_ctx: &VulkanContext,
        max_query_count: u32,
        log_tag: &str,
    ) -> Option<Self> {
        if max_query_count == 0 {
            return None;
        }

        let properties = unsafe {
            vulkan_ctx
                .instance()
                .as_raw()
                .get_physical_device_properties(vulkan_ctx.physical_device().as_raw())
        };
        if properties.limits.timestamp_compute_and_graphics != vk::TRUE {
            log::debug!("[{log_tag}] disabled: timestamp_compute_and_graphics unsupported");
            return None;
        }
        if properties.limits.timestamp_period <= 0.0 {
            log::debug!(
                "[{log_tag}] disabled: timestamp_period={}ns",
                properties.limits.timestamp_period
            );
            return None;
        }

        let create_info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(max_query_count);
        let device = vulkan_ctx.device().clone();
        let query_pool = match unsafe { device.create_query_pool(&create_info, None) } {
            Ok(pool) => pool,
            Err(err) => {
                log::warn!("[{log_tag}] disabled: query pool create failed: {err}");
                return None;
            }
        };

        Some(Self {
            device,
            query_pool,
            timestamp_period_ns: properties.limits.timestamp_period,
            max_query_count,
        })
    }

    pub fn timestamp_period_ns(&self) -> f32 {
        self.timestamp_period_ns
    }

    pub fn max_query_count(&self) -> u32 {
        self.max_query_count
    }

    pub fn record_reset(&self, cmdbuf: &CommandBuffer, query_count: u32) {
        self.record_reset_range(cmdbuf, 0, query_count);
    }

    pub fn record_reset_range(&self, cmdbuf: &CommandBuffer, first_query: u32, query_count: u32) {
        debug_assert!(first_query <= self.max_query_count);
        debug_assert!(first_query.saturating_add(query_count) <= self.max_query_count);
        unsafe {
            self.device.cmd_reset_query_pool(
                cmdbuf.as_raw(),
                self.query_pool,
                first_query,
                query_count,
            );
        }
    }

    pub fn record_timestamp(
        &self,
        cmdbuf: &CommandBuffer,
        stage: PipelineStage,
        query_index: u32,
    ) {
        debug_assert!(query_index < self.max_query_count);
        unsafe {
            self.device.cmd_write_timestamp(
                cmdbuf.as_raw(),
                stage.as_raw(),
                self.query_pool,
                query_index,
            );
        }
    }

    pub fn read_u64(&self, query_count: u32) -> VkResult<Vec<u64>> {
        self.read_u64_range(0, query_count)
    }

    pub fn read_u64_range(&self, first_query: u32, query_count: u32) -> VkResult<Vec<u64>> {
        debug_assert!(first_query <= self.max_query_count);
        debug_assert!(first_query.saturating_add(query_count) <= self.max_query_count);
        let mut timestamps = vec![0_u64; query_count as usize];
        unsafe {
            self.device.get_query_pool_results(
                self.query_pool,
                first_query,
                &mut timestamps,
                vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
            )?;
        }
        Ok(timestamps)
    }

    pub fn try_read_u64(&self, query_count: u32) -> VkResult<Option<Vec<u64>>> {
        self.try_read_u64_range(0, query_count)
    }

    pub fn try_read_u64_range(
        &self,
        first_query: u32,
        query_count: u32,
    ) -> VkResult<Option<Vec<u64>>> {
        debug_assert!(first_query <= self.max_query_count);
        debug_assert!(first_query.saturating_add(query_count) <= self.max_query_count);
        let mut timestamps = vec![0_u64; query_count as usize];
        let result = unsafe {
            self.device.get_query_pool_results(
                self.query_pool,
                first_query,
                &mut timestamps,
                vk::QueryResultFlags::TYPE_64,
            )
        };
        match result {
            Ok(()) => Ok(Some(timestamps)),
            Err(vk::Result::NOT_READY) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

//! Bounded, off-side publication of immutable GPU inputs. One temporary upload
//! buffer; shader reads never depend on a host-visible heap's available space.
use anyhow::{ensure, Context, Result};
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, CommandBuffer, MemoryLocation, VulkanContext,
};

pub(super) fn byte_len(parts: &[&[u8]]) -> Result<usize> {
    parts.iter().try_fold(0usize, |sum, part| {
        ensure!(part.len() % 4 == 0, "unaligned tree upload");
        sum.checked_add(part.len())
            .context("tree staging size overflow")
    })
}

pub(super) fn upload(
    context: &VulkanContext,
    allocator: &Allocator,
    parts: &[(&Buffer, &[u8])],
) -> Result<()> {
    let bytes = byte_len(&parts.iter().map(|(_, data)| *data).collect::<Vec<_>>())?;
    for (buffer, data) in parts {
        ensure!(
            data.len() as u64 <= buffer.get_size_bytes(),
            "tree upload exceeds destination"
        );
    }
    if bytes == 0 {
        return Ok(());
    }
    let staging = Buffer::try_new_sized(
        context.device().clone(),
        allocator.clone(),
        BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_SRC),
        MemoryLocation::CpuToGpu,
        bytes as u64,
    )?;
    let mut offset = 0;
    for (_, data) in parts {
        staging.fill_range_with_raw_u8(offset, data)?;
        offset += data.len() as u64;
    }
    let cmdbuf = CommandBuffer::new(context.device(), context.command_pool());
    cmdbuf.begin(true);
    offset = 0;
    for (buffer, data) in parts {
        if !data.is_empty() {
            staging.record_copy_to_buffer(&cmdbuf, buffer, data.len() as u64, offset, 0);
        }
        offset += data.len() as u64;
    }
    cmdbuf.end();
    let job = cmdbuf.submit_gpu_job(&context.get_general_queue(), "tree_static_upload")?;
    job.wait_complete()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staging_accounting_rejects_unaligned_input_and_ignores_empty_parts() {
        assert_eq!(byte_len(&[&[], &[0; 4], &[0; 8]]).unwrap(), 12);
        assert!(byte_len(&[&[0; 3]]).is_err());
    }
}

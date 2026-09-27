//! Opaque, record-addressed GPU storage. No models, views, frames of animation,
//! cache policy or total byte budget live here. A logical allocation can span
//! many buffers; shaders use one small handle rather than descriptor arrays.
//!
//! `begin_frame(slot)` requires that slot's submission fence to have completed.
//! Before recording a shader use, call `use_in_frame`: it both declares pointer-
//! reachable resources (reflection cannot see them) and retains residency until
//! that slot is completed. Dropping/replacing the caller's allocation is safe.
use crate::{vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, Device,
    MemoryLocation, VulkanContext};
use anyhow::{ensure, Context, Result};
use bytemuck::{Pod, Zeroable};
use std::{alloc::Layout, collections::HashMap, sync::Arc};

// Allocation granularity, NOT a logical-allocation or total-storage cap.
const TARGET_BLOCK_BYTES: u64 = 64 * 1024 * 1024;

/// Shader ABI for gpu_paged_storage.slang. Address/layout fields stay opaque to
/// callers. A copied GPU handle is valid only while its allocation is resident.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuStorageHandle {
    page_table: u64,
    len: u64,
    stride: u32,
    page_shift: u32,
}
#[derive(Clone, Copy, Debug)]
struct Plan { count: u64, stride: u32, alignment: u64, page_shift: u32, pages: usize }
impl Plan {
    fn new(count: u64, layout: Layout, max_allocation: u64) -> Result<Self> {
        ensure!(layout.size()>0 && layout.size()%layout.align()==0,
            "GPU record stride must be nonzero and a multiple of alignment");
        let stride=u32::try_from(layout.size()).context("GPU record stride exceeds shader address format")?;
        let alignment=layout.align() as u64;
        count.checked_mul(u64::from(stride)).context("GPU storage byte count overflow")?;
        let usable=max_allocation.checked_sub(alignment-1).context("record alignment exceeds device allocation limit")?;
        ensure!(u64::from(stride)<=usable,"one GPU record exceeds device allocation limit");
        let records=(TARGET_BLOCK_BYTES.max(u64::from(stride)).min(usable))/u64::from(stride);
        let page_shift=63-records.leading_zeros();
        let pages=usize::try_from(count.div_ceil(1u64<<page_shift)).context("GPU page count exceeds host address space")?;
        let table_bytes=(pages as u64).checked_mul(8).context("GPU page-table byte count overflow")?;
        ensure!(table_bytes<=max_allocation,"GPU page table exceeds actual device allocation limit");
        Ok(Self{count,stride,alignment,page_shift,pages})
    }
    fn page_bytes(self, page: usize) -> u64 {
        let records=(self.count-((page as u64)<<self.page_shift)).min(1u64<<self.page_shift);
        records*u64::from(self.stride)+self.alignment-1
    }
}
struct AllocationInner { handle:GpuStorageHandle, table:Option<Buffer>, blocks:Vec<Buffer>, bytes:u64 }
#[derive(Clone)]
pub struct GpuStorageAllocation(Arc<AllocationInner>);
impl GpuStorageAllocation {
    pub fn handle(&self)->GpuStorageHandle {self.0.handle}
    pub fn block_count(&self)->usize {self.0.blocks.len()}
    pub fn resident_bytes(&self)->u64 {self.0.bytes}
}
#[derive(Default)]
struct Residency<T> { frames:Vec<HashMap<usize,T>> }
impl<T:Clone> Residency<T> {
    fn begin(&mut self,slot:usize) {
        while self.frames.len()<=slot {self.frames.push(HashMap::new());}
        self.frames[slot].clear();
    }
    fn retain(&mut self,slot:usize,id:usize,value:&T) {
        self.frames.get_mut(slot).expect("begin GPU storage frame before recording uses")
            .entry(id).or_insert_with(||value.clone());
    }
}
pub struct GpuPagedStorage {
    device:Device, allocator:Allocator, max_allocation:u64,
    residency:Residency<GpuStorageAllocation>,
}
impl GpuPagedStorage {
    pub fn new(context:&VulkanContext,allocator:Allocator)->Self {
        // Vulkan 1.1 property, available on all this renderer's BDA-capable devices.
        let mut maintenance=vk::PhysicalDeviceMaintenance3Properties::default();
        let mut properties=vk::PhysicalDeviceProperties2::default().push_next(&mut maintenance);
        unsafe {context.instance().get_physical_device_properties2(context.physical_device().as_raw(),&mut properties);}
        Self {device:context.device().clone(),allocator,max_allocation:maintenance.max_memory_allocation_size,
            residency:Residency{frames:Vec::new()}}
    }
    /// Allocate uninitialized records. Every read must follow initialization.
    /// Failure leaves no partially published allocation and never changes policy.
    pub fn allocate(&self,count:u64,layout:Layout)->Result<GpuStorageAllocation> {
        let plan=Plan::new(count,layout,self.max_allocation)?;
        let create=|bytes,location| Buffer::try_new_sized(self.device.clone(),self.allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER|vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS),location,bytes);
        let mut blocks=Vec::new();let mut addresses=Vec::new();
        blocks.try_reserve_exact(plan.pages).context("GPU block ownership allocation failed")?;
        addresses.try_reserve_exact(plan.pages).context("GPU page table allocation failed")?;
        let mut bytes=0u64;
        for page in 0..plan.pages {
            let size=plan.page_bytes(page);
            let block=create(size,MemoryLocation::GpuOnly)
                .with_context(||format!("GPU paged storage: allocating block {page}/{} for {count} records of {} bytes; insufficient resources is an error, not a rendering fallback",plan.pages,plan.stride))?;
            let base=block.device_address();
            ensure!(base!=0,"device returned a null GPU buffer address");
            let aligned=base.checked_add(plan.alignment-1).context("GPU address alignment overflow")? & !(plan.alignment-1);
            addresses.push(aligned);blocks.push(block);bytes=bytes.checked_add(size).context("GPU resident byte count overflow")?;
        }
        let table=if addresses.is_empty(){None}else{
            let size=addresses.len() as u64*8;
            let table=create(size,MemoryLocation::CpuToGpu)?;
            table.fill(&addresses)?;bytes=bytes.checked_add(size).context("GPU resident byte count overflow")?;
            Some(table)
        };
        let handle=GpuStorageHandle {page_table:table.as_ref().map_or(0,Buffer::device_address),
            len:count,stride:plan.stride,page_shift:plan.page_shift};
        Ok(GpuStorageAllocation(Arc::new(AllocationInner{handle,table,blocks,bytes})))
    }
    /// Diagnostic payload + page-table bytes retained by all submitted/active
    /// frame slots, counting shared allocations once (not allocator heap size).
    pub fn resident_bytes(&self) -> u64 {
        self.residency.frames.iter().flat_map(|frame| frame.iter())
            .collect::<HashMap<_, _>>().values().map(|a| a.resident_bytes()).sum()
    }
    pub fn begin_frame(&mut self,slot:usize) {self.residency.begin(slot);}
    pub fn use_in_frame(&mut self,slot:usize,cmd:&CommandBuffer,allocation:&GpuStorageAllocation,usage:BufferUse) {
        self.residency.retain(slot,Arc::as_ptr(&allocation.0) as usize,allocation);
        // The page table is read even when the payload is being written.
        if let Some(table)=&allocation.0.table {cmd.use_buffer(table,BufferUse::ShaderRead);}
        for block in &allocation.0.blocks {cmd.use_buffer(block,usage);}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn large_logical_allocations_have_no_128_mib_budget() {
        for bytes in [0,128<<20,256<<20,4u64<<30,9u64<<30] {
            let plan=Plan::new(bytes/32,Layout::from_size_align(32,16).unwrap(),1<<30).unwrap();
            assert_eq!(plan.pages as u64,(bytes/32).div_ceil(1<<plan.page_shift));
            let payload=(0..plan.pages).map(|i|plan.page_bytes(i)-15).sum::<u64>();
            assert_eq!(payload,bytes);
            assert!((0..plan.pages).all(|i|plan.page_bytes(i)<=(64<<20)+15));
        }
    }
    #[test] fn every_record_resolves_inside_one_aligned_block_including_partial_tail() {
        for (stride,alignment) in [(12,4),(32,16),(40,8),(256,256)] {
            let plan=Plan::new(5_000_007,Layout::from_size_align(stride,alignment).unwrap(),1<<30).unwrap();
            let records=1u64<<plan.page_shift;
            for page in 0..plan.pages {
                let start=(page as u64)*records;
                let last=(start+records).min(plan.count)-1;
                for index in [start,last] {
                    let offset=(index & (records-1))*u64::from(plan.stride);
                    assert_eq!(index>>plan.page_shift,page as u64);
                    assert!(offset+u64::from(plan.stride)<=plan.page_bytes(page));
                    assert_eq!(offset%alignment as u64,0);
                }
            }
        }
    }
    #[test] fn actual_device_limits_and_overflow_are_errors() {
        assert!(Plan::new(u64::MAX,Layout::from_size_align(32,16).unwrap(),1<<30).is_err());
        assert!(Plan::new(1,Layout::from_size_align(1024,16).unwrap(),512).is_err());
        let plan=Plan::new(500,Layout::from_size_align(32,16).unwrap(),4096).unwrap();
        assert!((0..plan.pages).all(|i|plan.page_bytes(i)<=4096));
        assert_eq!(std::mem::size_of::<GpuStorageHandle>(),24);
        assert_eq!(std::mem::offset_of!(GpuStorageHandle,stride),16);
    }
    #[test] fn replacement_survives_until_every_consuming_frame_completes() {
        let mut leases=Residency{frames:Vec::new()};
        let old=Arc::new(());let weak=Arc::downgrade(&old);
        leases.begin(0);leases.retain(0,1,&old);leases.retain(0,1,&old);
        leases.begin(1);leases.retain(1,1,&old);
        assert_eq!(Arc::strong_count(&old),3);
        drop(old);leases.begin(0);assert!(weak.upgrade().is_some());
        leases.begin(1);assert!(weak.upgrade().is_none());
    }
}

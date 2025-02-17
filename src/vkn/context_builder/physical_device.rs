use std::ffi::CStr;

use ash::{
    khr::{surface, swapchain},
    vk,
};

use crate::vkn::context::QueueFamilyIndices;

// Example device info for scoring / printing
#[derive(Debug)]
pub struct DeviceInfo {
    pub device: vk::PhysicalDevice,
    pub score: i32,
    pub total_memory: f64,
    pub device_name: String,
    pub device_type: vk::PhysicalDeviceType,
}

// TODO: also print status of queues

fn print_all_devices_stats(device_infos: &[DeviceInfo]) {
    use comfy_table::Table;

    let mut table = Table::new();
    table.set_header(vec!["Device", "Type", "Memory (MB)", "Score"]);

    for info in device_infos {
        table.add_row(vec![
            info.device_name.clone(),
            format!("{:?}", info.device_type),
            format!("{:.2}", info.total_memory),
            format!("{}", info.score),
        ]);
    }

    println!("{}", table);
}

/// Checks whether a device supports the swapchain extension
fn device_supports_required_extensions(
    instance: &ash::Instance,
    device: vk::PhysicalDevice,
) -> bool {
    let extension_props = unsafe {
        instance
            .enumerate_device_extension_properties(device)
            .expect("Failed to get device extension properties")
    };

    // if more extensions are required, add them here
    let required_extensions = vec![swapchain::NAME];

    for required_ext in required_extensions.iter() {
        let required_found = extension_props.iter().any(|ext| {
            let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
            name == *required_ext
        });
        if !required_found {
            return false;
        }
    }

    true
}

/// Collects all candidate queue-family indices for GRAPHICS, PRESENT, and COMPUTE.
fn gather_queue_family_candidates(
    instance: &ash::Instance,
    surface_loader: &surface::Instance,
    surface_khr: vk::SurfaceKHR,
    device: vk::PhysicalDevice,
) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let props = unsafe { instance.get_physical_device_queue_family_properties(device) };

    let mut graphics_candidates = vec![];
    let mut present_candidates = vec![];
    let mut compute_candidates = vec![];

    for (idx, family) in props.iter().enumerate() {
        // We ignore families with zero queue_count
        if family.queue_count == 0 {
            continue;
        }
        let index = idx as u32;

        if family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            graphics_candidates.push(index);
        }

        if family.queue_flags.contains(vk::QueueFlags::COMPUTE) {
            compute_candidates.push(index);
        }

        let present_support = unsafe {
            surface_loader
                .get_physical_device_surface_support(device, index, surface_khr)
                .expect("Failed to get surface support")
        };
        if present_support {
            present_candidates.push(index);
        }
    }

    (graphics_candidates, present_candidates, compute_candidates)
}

/// Tries to pick distinct queue-family indices for GRAPHICS, PRESENT, and COMPUTE
/// from the candidate lists. If not possible, tries its best to share the least
/// amount of queues.
fn pick_best_queue_family_indices(
    gfx_candidates: &[u32],
    present_candidates: &[u32],
    compute_candidates: &[u32],
) -> Option<QueueFamilyIndices> {
    if gfx_candidates.is_empty() || present_candidates.is_empty() || compute_candidates.is_empty() {
        return None;
    }

    // if we can find three distinct indices, do it
    for &gfx_index in gfx_candidates {
        for &present_index in present_candidates {
            for &compute_index in compute_candidates {
                if gfx_index != present_index
                    && gfx_index != compute_index
                    && present_index != compute_index
                {
                    return Some(QueueFamilyIndices {
                        graphics: gfx_index,
                        present: present_index,
                        compute: compute_index,
                    });
                }
            }
        }
    }

    /// Tries to distanct 2 queue-families at least
    fn try_distinct(candidates_list_1: &[u32], candidates_list_2: &[u32]) -> Option<(u32, u32)> {
        for &index1 in candidates_list_1 {
            for &index2 in candidates_list_2 {
                if index1 != index2 {
                    return Some((index1, index2));
                }
            }
        }
        None
    }

    if let Some((gfx_index, present_index)) = try_distinct(gfx_candidates, present_candidates) {
        return Some(QueueFamilyIndices {
            graphics: gfx_index,
            present: present_index,
            compute: compute_candidates.get(0).unwrap().clone(),
        });
    }

    if let Some((gfx_index, compute_index)) = try_distinct(gfx_candidates, compute_candidates) {
        return Some(QueueFamilyIndices {
            graphics: gfx_index,
            present: present_candidates.get(0).unwrap().clone(),
            compute: compute_index,
        });
    }

    if let Some((present_index, compute_index)) =
        try_distinct(present_candidates, compute_candidates)
    {
        return Some(QueueFamilyIndices {
            graphics: gfx_candidates.get(0).unwrap().clone(),
            present: present_index,
            compute: compute_index,
        });
    }

    // fallback option: picking the first from each list
    Some(QueueFamilyIndices {
        graphics: gfx_candidates.get(0).unwrap().clone(),
        present: present_candidates.get(0).unwrap().clone(),
        compute: compute_candidates.get(0).unwrap().clone(),
    })
}

/// Create a Vulkan physical device, picking the best device that supports
/// GRAPHICS, PRESENT, and COMPUTE, as well as the swapchain extension.
///
/// Then find the best queue-family indices (trying to maximize distinctness).
///
pub fn create_physical_device(
    instance: &ash::Instance,
    surface_loader: &surface::Instance,
    surface_khr: vk::SurfaceKHR,
) -> (vk::PhysicalDevice, QueueFamilyIndices) {
    let devices = unsafe {
        instance
            .enumerate_physical_devices()
            .expect("Failed to enumerate physical devices")
    };

    // build a list of device infos for printing / scoring
    let mut device_infos = vec![];
    for &dev in devices.iter() {
        // check if it supports required extensions
        if !device_supports_required_extensions(instance, dev) {
            continue;
        }

        let (gfx_candidates, present_candidates, compute_candidates) =
            gather_queue_family_candidates(instance, surface_loader, surface_khr, dev);

        if gfx_candidates.is_empty()
            || present_candidates.is_empty()
            || compute_candidates.is_empty()
        {
            continue;
        }

        // get props for device type and name
        let props = unsafe { instance.get_physical_device_properties(dev) };
        let device_name = unsafe {
            CStr::from_ptr(props.device_name.as_ptr())
                .to_string_lossy()
                .into_owned()
        };
        let device_type = props.device_type;

        let mem_props = unsafe { instance.get_physical_device_memory_properties(dev) };
        // Sum all device-local heaps as a simple approximation (in MB)
        let mut total_vram = 0u64;
        for i in 0..mem_props.memory_heap_count {
            let heap = mem_props.memory_heaps[i as usize];
            if heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL) {
                total_vram += heap.size;
            }
        }
        let total_memory_mb = (total_vram as f64) / (1024.0 * 1024.0);

        // Compute a rough score
        // Discrete GPU: +100, Integrated GPU: +50, others: +10
        // Then add memory-based bonus
        let gpu_type_score = match device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => 100,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 50,
            _ => 10,
        };
        // Maybe give 1 point per 256 MB
        let mem_score = (total_memory_mb / 256.0).floor() as i32;

        let score = gpu_type_score + mem_score;

        device_infos.push(DeviceInfo {
            device: dev,
            score,
            total_memory: total_memory_mb,
            device_name,
            device_type,
        });
    }

    print_all_devices_stats(&device_infos);

    // sort descending by score
    device_infos.sort_by(|a, b| b.score.cmp(&a.score));

    // pick the device with the highest score that can also pick distinct queues
    for info in &device_infos {
        let (gfx_candidates, present_candidates, compute_candidates) =
            gather_queue_family_candidates(instance, surface_loader, surface_khr, info.device);

        if let Some(q_indices) = pick_best_queue_family_indices(
            &gfx_candidates,
            &present_candidates,
            &compute_candidates,
        ) {
            // found the best device with a valid set of indices
            unsafe {
                let props = instance.get_physical_device_properties(info.device);
                let chosen_name = CStr::from_ptr(props.device_name.as_ptr());
                log::info!("Selected physical device: {:?}", chosen_name);
            }
            return (info.device, q_indices);
        }
    }

    panic!("Could not find any suitable device with the required queue families and extensions.");
}

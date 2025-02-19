use ash::{
    khr::{surface, swapchain},
    vk::{self},
};
use std::ffi::CStr;

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

fn print_all_devices_with_selection(device_infos: &[DeviceInfo], selection_idx: usize) {
    let mut table = comfy_table::Table::new();
    table.set_header(vec!["Device", "Type", "Memory (MB)", "Score", "Selected?"]);

    for (idx, info) in device_infos.iter().enumerate() {
        table.add_row(vec![
            info.device_name.clone(),
            format!("{:?}", info.device_type),
            format!("{:.2}", info.total_memory),
            format!("{}", info.score),
            if idx == selection_idx {
                "yes".to_string()
            } else {
                "".to_string()
            },
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

/// Prints detailed information about each queue family of the given physical device.
fn print_queue_family_info(
    instance: &ash::Instance,
    device: vk::PhysicalDevice,
    queue_family_index_candidates: &QueueFamilyIndexCandidates,
) {
    let queue_families = unsafe { instance.get_physical_device_queue_family_properties(device) };

    let mut table = comfy_table::Table::new();
    table.set_header(vec![
        "Queue Family Index",
        "Graphics",
        "Present",
        "Compute",
        "Transfer",
        "Sparse Binding",
    ]);

    for (index, _queue_family) in queue_families.iter().enumerate() {
        let q_index = index as u32;

        // Determine which operations this queue family supports
        let supports_graphics = if queue_family_index_candidates.graphics.contains(&q_index) {
            "Yes"
        } else {
            "No"
        };

        let supports_present = if queue_family_index_candidates.present.contains(&q_index) {
            "Yes"
        } else {
            "No"
        };

        let supports_compute = if queue_family_index_candidates.compute.contains(&q_index) {
            "Yes"
        } else {
            "No"
        };

        let supports_transfer = if queue_family_index_candidates.transfer.contains(&q_index) {
            "Yes"
        } else {
            "No"
        };

        let supports_sparse_binding = if queue_family_index_candidates
            .sparse_binding
            .contains(&q_index)
        {
            "Yes"
        } else {
            "No"
        };

        table.add_row(vec![
            q_index.to_string(),
            supports_graphics.to_string(),
            supports_present.to_string(),
            supports_compute.to_string(),
            supports_transfer.to_string(),
            supports_sparse_binding.to_string(),
        ]);
    }

    println!("{}", table);
}

/// Prints a summary table of selected queue families for different operations.
fn print_selected_queue_families(qf_indices: &QueueFamilyIndices) {
    let mut table = comfy_table::Table::new();
    table.set_header(vec!["Queue Type", "Queue Family Index"]);

    table.add_row(vec!["General", &qf_indices.general.to_string()]);
    table.add_row(vec!["Transfer Only", &qf_indices.transfer_only.to_string()]);

    println!("{}", table);
}

struct QueueFamilyIndexCandidates {
    graphics: Vec<u32>,
    present: Vec<u32>,
    compute: Vec<u32>,
    transfer: Vec<u32>,
    sparse_binding: Vec<u32>,
}

impl QueueFamilyIndexCandidates {
    /// Returns true if all queues have at least one candidate
    fn is_valid(&self) -> bool {
        !self.graphics.is_empty()
            && !self.present.is_empty()
            && !self.compute.is_empty()
            && !self.transfer.is_empty()
            && !self.sparse_binding.is_empty()
    }
}

fn gather_queue_family_candidates(
    instance: &ash::Instance,
    surface_loader: &surface::Instance,
    surface_khr: vk::SurfaceKHR,
    device: vk::PhysicalDevice,
) -> QueueFamilyIndexCandidates {
    let props = unsafe { instance.get_physical_device_queue_family_properties(device) };

    let mut graphics = vec![];
    let mut present = vec![];
    let mut compute = vec![];
    let mut transfer = vec![];
    let mut sparse_binding = vec![];

    for (idx, family) in props.iter().enumerate() {
        // We ignore families with zero queue_count
        if family.queue_count == 0 {
            continue;
        }
        let index = idx as u32;

        if family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            graphics.push(index);
        }

        if family.queue_flags.contains(vk::QueueFlags::COMPUTE) {
            compute.push(index);
        }

        if family.queue_flags.contains(vk::QueueFlags::TRANSFER) {
            transfer.push(index);
        }

        if family.queue_flags.contains(vk::QueueFlags::SPARSE_BINDING) {
            sparse_binding.push(index);
        }

        let present_support = unsafe {
            surface_loader
                .get_physical_device_surface_support(device, index, surface_khr)
                .expect("Failed to get surface support")
        };
        if present_support {
            present.push(index);
        }
    }

    QueueFamilyIndexCandidates {
        graphics,
        present,
        compute,
        transfer,
        sparse_binding,
    }
}

fn pick_best_queue_family_indices(
    queue_family_index_candidates: &QueueFamilyIndexCandidates,
) -> Option<QueueFamilyIndices> {
    if !queue_family_index_candidates.is_valid() {
        return None;
    }

    // find candidates that support GRAPHICS + PRESENT + COMPUTE + TRANSFER
    let mut general_candidates = Vec::new();
    for &gfx_idx in &queue_family_index_candidates.graphics {
        if queue_family_index_candidates.present.contains(&gfx_idx)
            && queue_family_index_candidates.compute.contains(&gfx_idx)
            && queue_family_index_candidates.transfer.contains(&gfx_idx)
        {
            general_candidates.push(gfx_idx);
        }
    }
    if general_candidates.is_empty() {
        return None;
    }

    // maybe add more criteria here to pick the best general queue (queue count, etc.)
    let general_idx = general_candidates[0];

    // try to find a separate transfer-only queue that doesn't overlap with the general queue
    // this is a queue that supports TRANSFER but ideally doesn't also support GRAPHICS or COMPUTE.
    // if such a dedicated queue cannot be found, pick the same queue as general.
    let mut dedicated_transfer_candidates = Vec::new();
    for &transfer_idx in &queue_family_index_candidates.transfer {
        // we'll consider it "dedicated" if it doesn't appear in the graphics or compute lists.
        let is_dedicated = !queue_family_index_candidates
            .graphics
            .contains(&transfer_idx)
            && !queue_family_index_candidates
                .compute
                .contains(&transfer_idx);

        if is_dedicated {
            dedicated_transfer_candidates.push(transfer_idx);
        }
    }

    let transfer_only_idx = if !dedicated_transfer_candidates.is_empty() {
        // pick the first dedicated queue if it exists
        dedicated_transfer_candidates[0]
    } else {
        // otherwise, see if there’s at least a different queue in transfer:
        // this might not be "pure" transfer-only, but at least it’s separate
        let maybe_separate = queue_family_index_candidates
            .transfer
            .iter()
            .find(|&&idx| idx != general_idx);

        match maybe_separate {
            Some(&separate_transfer_idx) => separate_transfer_idx,
            None => {
                // Finally, fall back to the same queue as general if absolutely no separate queue is found.
                general_idx
            }
        }
    };

    Some(QueueFamilyIndices {
        general: general_idx,
        transfer_only: transfer_only_idx,
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

    // Build a list of device infos for printing / scoring
    let mut device_infos = vec![];
    for &dev in devices.iter() {
        // Check if it supports required extensions
        if !device_supports_required_extensions(instance, dev) {
            continue;
        }

        let queue_family_candidates =
            gather_queue_family_candidates(instance, surface_loader, surface_khr, dev);

        if !queue_family_candidates.is_valid() {
            continue;
        }

        // Get props for device type and name
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

        // Discrete GPU: +100, Integrated GPU: +50, others: +10
        // Then add memory-based bonus: 1 point per 256 MB
        let gpu_type_score = match device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => 100,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 50,
            _ => 10,
        };

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

    // Sort descending by score
    device_infos.sort_by(|a, b| b.score.cmp(&a.score));

    print_all_devices_with_selection(&device_infos, 0);

    // Pick the device with the highest score that can also pick distinct queues
    let info = device_infos.get(0).expect("No suitable device found");

    let queue_family_index_candidates =
        gather_queue_family_candidates(instance, surface_loader, surface_khr, info.device);

    print_queue_family_info(instance, info.device, &queue_family_index_candidates);

    let queue_family_indices = pick_best_queue_family_indices(&queue_family_index_candidates)
        .expect("Cannot find suitable queue families for the best device");

    // **New Step:** Print the selected queue families
    print_selected_queue_families(&queue_family_indices);

    // Found the best device with a valid set of indices
    unsafe {
        let props = instance.get_physical_device_properties(info.device);
        let chosen_name = CStr::from_ptr(props.device_name.as_ptr());
        log::info!("Selected physical device: {:?}", chosen_name);
    }
    return (info.device, queue_family_indices);
}

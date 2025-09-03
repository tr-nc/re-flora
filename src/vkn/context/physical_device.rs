use ash::{
    khr::swapchain,
    vk::{self},
};
use std::ffi::CStr;

use super::{instance::Instance, queue::QueueFamilyIndices, surface::Surface};

#[derive(Clone, Copy)]
pub struct PhysicalDevice {
    pub device: vk::PhysicalDevice,
}

impl PhysicalDevice {
    pub fn new(instance: &Instance, surface: &Surface) -> (Self, QueueFamilyIndices) {
        let (device, queue_family_indices) = create_physical_device(
            instance.as_raw(),
            &surface.surface_instance(),
            surface.surface_khr(),
        );
        (Self { device }, queue_family_indices)
    }

    pub fn as_raw(&self) -> vk::PhysicalDevice {
        self.device
    }
}

// example device info for scoring / printing
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub device: vk::PhysicalDevice,
    pub score: i32,
    pub total_memory: f64,
    pub device_name: String,
    pub device_type: vk::PhysicalDeviceType,
}

fn print_all_devices_with_selection(device_infos: &[DeviceInfo], selection_idx: usize) {
    println!("\n--- Suitable Physical Devices ---");
    let mut table = comfy_table::Table::new();
    table.set_header(vec!["Device", "Type", "Memory (MB)", "Score", "Selected?"]);

    for (idx, info) in device_infos.iter().enumerate() {
        table.add_row(vec![
            info.device_name.clone(),
            format!("{:?}", info.device_type),
            format!("{:.2}", info.total_memory),
            format!("{}", info.score),
            if idx == selection_idx {
                "Yes".to_string()
            } else {
                "".to_string()
            },
        ]);
    }

    println!("{}", table);
}

/// Checks for required device extensions and returns a list of any that are missing.
fn get_missing_required_extensions(
    instance: &ash::Instance,
    device: vk::PhysicalDevice,
) -> Vec<&'static CStr> {
    let extension_props = unsafe {
        instance
            .enumerate_device_extension_properties(device)
            .expect("Failed to get device extension properties")
    };

    // If more extensions are required in the future, add them to this list.
    let required_extensions = vec![swapchain::NAME];
    let mut missing = Vec::new();

    for &required_ext in required_extensions.iter() {
        let is_found = extension_props.iter().any(|ext| {
            let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
            name == required_ext
        });
        if !is_found {
            missing.push(required_ext);
        }
    }

    missing
}

/// Prints detailed information about each queue family of the given physical device.
fn print_queue_family_info(
    instance: &ash::Instance,
    device: vk::PhysicalDevice,
    queue_family_index_candidates: &QueueFamilyIndexCandidates,
) {
    let queue_families = unsafe { instance.get_physical_device_queue_family_properties(device) };

    println!("\n--- Queue Family Analysis for Selected Device ---");
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

        table.add_row(vec![
            q_index.to_string(),
            if queue_family_index_candidates.graphics.contains(&q_index) {
                "Yes"
            } else {
                ""
            }
            .to_string(),
            if queue_family_index_candidates.present.contains(&q_index) {
                "Yes"
            } else {
                ""
            }
            .to_string(),
            if queue_family_index_candidates.compute.contains(&q_index) {
                "Yes"
            } else {
                ""
            }
            .to_string(),
            if queue_family_index_candidates.transfer.contains(&q_index) {
                "Yes"
            } else {
                ""
            }
            .to_string(),
            if queue_family_index_candidates
                .sparse_binding
                .contains(&q_index)
            {
                "Yes"
            } else {
                ""
            }
            .to_string(),
        ]);
    }

    println!("{}", table);
}

/// Prints a summary table of selected queue families for different operations.
fn print_selected_queue_families(qf_indices: &QueueFamilyIndices) {
    println!("\n--- Selected Queue Family Indices ---");
    let mut table = comfy_table::Table::new();
    table.set_header(vec!["Queue Type", "Queue Family Index"]);

    table.add_row(vec![
        "General (Graphics, Present, Compute)",
        &qf_indices.general.to_string(),
    ]);
    table.add_row(vec![
        "Dedicated Transfer (if available)",
        &qf_indices.transfer_only.to_string(),
    ]);

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
    /// Returns true if all required queue types have at least one candidate family.
    fn is_complete(&self) -> bool {
        // Here, we define what makes a device "complete" in terms of queue support.
        // For this application, we need Graphics, Present, and Compute.
        // Transfer and Sparse Binding are checked but not strictly required for a device to be considered viable.
        !self.graphics.is_empty() && !self.present.is_empty() && !self.compute.is_empty()
    }
}

fn gather_queue_family_candidates(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
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
                .unwrap_or(false) // Assume no support on error
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
    // This function assumes `is_complete()` has already been checked.
    // We now find the best combination of queues.

    // Find candidates that support GRAPHICS + PRESENT + COMPUTE + TRANSFER.
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
        return None; // No single queue family supports all required operations.
    }

    // For simplicity, we pick the first general-purpose queue found.
    // More advanced logic could score based on queue count or other properties.
    let general_idx = general_candidates[0];

    // Now, try to find a dedicated transfer-only queue.
    // A "dedicated" queue is one that supports TRANSFER but not GRAPHICS or COMPUTE.
    // This is optimal for offloading memory transfers from the main rendering/compute queue.
    let dedicated_transfer_candidates: Vec<u32> = queue_family_index_candidates
        .transfer
        .iter()
        .filter(|&&idx| {
            !queue_family_index_candidates.graphics.contains(&idx)
                && !queue_family_index_candidates.compute.contains(&idx)
        })
        .cloned()
        .collect();

    let transfer_only_idx = if !dedicated_transfer_candidates.is_empty() {
        // Prefer a truly dedicated transfer queue.
        dedicated_transfer_candidates[0]
    } else {
        // If not found, try to find any transfer queue that is different from the general one.
        // This still provides some potential for parallelism.
        queue_family_index_candidates
            .transfer
            .iter()
            .find(|&&idx| idx != general_idx)
            .cloned()
            .unwrap_or(general_idx) // Fallback: use the general queue if no other option exists.
    };

    Some(QueueFamilyIndices {
        general: general_idx,
        transfer_only: transfer_only_idx,
    })
}

/// Evaluates all physical devices, prints a detailed report, and then selects the best one.
///
/// This function performs the following steps:
/// 1. Enumerates all physical devices available on the system.
/// 2. For each device, it gathers properties, checks for required extensions (like swapchain),
///    and analyzes queue family support.
/// 3. It prints a comprehensive table showing every device and the reason it was deemed
///    suitable or unsuitable.
/// 4. It filters the list to only suitable devices, scores them (Discrete > Integrated > Other),
///    and sorts them to find the best candidate.
/// 5. Finally, it selects the best device and determines the optimal queue family indices,
///    preferring dedicated queues for transfer operations where possible.
pub fn create_physical_device(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface_khr: vk::SurfaceKHR,
) -> (vk::PhysicalDevice, QueueFamilyIndices) {
    // A temporary struct to hold evaluation data for all devices.
    struct DeviceEvaluation {
        device_info: DeviceInfo,
        missing_extensions: Vec<&'static CStr>,
        queue_families_complete: bool,
        has_all_purpose_queue: bool,
    }

    // A helper function to print the detailed evaluation report.
    fn print_device_evaluation_table(evaluations: &[DeviceEvaluation]) {
        println!("--- Physical Device Evaluation Report ---");
        let mut table = comfy_table::Table::new();
        table.set_header(vec![
            "Device",
            "Type",
            "Memory (MB)",
            "Suitability",
            "Reason",
        ]);

        if evaluations.is_empty() {
            table.add_row(vec!["No Vulkan-capable physical devices found.".to_string()]);
        } else {
            for eval in evaluations {
                let mut reason = String::new();
                let is_suitable = eval.missing_extensions.is_empty()
                    && eval.queue_families_complete
                    && eval.has_all_purpose_queue;

                if !eval.missing_extensions.is_empty() {
                    let missing_ext_str: String = eval
                        .missing_extensions
                        .iter()
                        .map(|s| s.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(", ");
                    reason.push_str(&format!("Missing extensions: [{}]. ", missing_ext_str));
                }
                if !eval.queue_families_complete {
                    reason.push_str(
                        "Missing essential queue support (Graphics, Present, or Compute). ",
                    );
                }
                if eval.queue_families_complete && !eval.has_all_purpose_queue {
                    reason.push_str("No single queue family supports Graphics, Present, and Compute simultaneously. ");
                }

                table.add_row(vec![
                    eval.device_info.device_name.clone(),
                    format!("{:?}", eval.device_info.device_type),
                    format!("{:.2}", eval.device_info.total_memory),
                    if is_suitable {
                        "Suitable"
                    } else {
                        "Not Suitable"
                    }
                    .to_string(),
                    if reason.is_empty() {
                        "All requirements met.".to_string()
                    } else {
                        reason
                    },
                ]);
            }
        }
        println!("{}", table);
    }

    let devices = unsafe {
        instance
            .enumerate_physical_devices()
            .expect("Failed to enumerate physical devices")
    };

    // 1. Evaluate every device and collect detailed information.
    let evaluations: Vec<DeviceEvaluation> = devices
        .iter()
        .map(|&dev| {
            let props = unsafe { instance.get_physical_device_properties(dev) };
            let mem_props = unsafe { instance.get_physical_device_memory_properties(dev) };

            let device_name = unsafe {
                CStr::from_ptr(props.device_name.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            };
            let device_type = props.device_type;

            let total_vram: u64 = mem_props.memory_heaps[..mem_props.memory_heap_count as usize]
                .iter()
                .filter(|heap| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
                .map(|heap| heap.size)
                .sum();
            let total_memory_mb = (total_vram as f64) / (1024.0 * 1024.0);

            let gpu_type_score = match device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => 100,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 50,
                _ => 10,
            };
            let mem_score = (total_memory_mb / 256.0).floor() as i32;
            let score = gpu_type_score + mem_score;

            let missing_extensions = get_missing_required_extensions(instance, dev);
            let queue_family_candidates =
                gather_queue_family_candidates(instance, surface_loader, surface_khr, dev);
            let queue_families_complete = queue_family_candidates.is_complete();
            let has_all_purpose_queue =
                pick_best_queue_family_indices(&queue_family_candidates).is_some();

            DeviceEvaluation {
                device_info: DeviceInfo {
                    device: dev,
                    score,
                    total_memory: total_memory_mb,
                    device_name,
                    device_type,
                },
                missing_extensions,
                queue_families_complete,
                has_all_purpose_queue,
            }
        })
        .collect();

    // 2. Print the detailed report for the user.
    print_device_evaluation_table(&evaluations);

    // 3. Filter down to only suitable devices.
    let mut suitable_devices: Vec<DeviceInfo> = evaluations
        .into_iter()
        .filter(|eval| {
            eval.missing_extensions.is_empty()
                && eval.queue_families_complete
                && eval.has_all_purpose_queue
        })
        .map(|eval| eval.device_info)
        .collect();

    // 4. If no devices are suitable, panic with a helpful message.
    if suitable_devices.is_empty() {
        panic!("No suitable physical device found. See the evaluation report above for details on why each device was rejected.");
    }

    // 5. Sort suitable devices by score to find the best one.
    suitable_devices.sort_by(|a, b| b.score.cmp(&a.score));

    // Print the filtered list of suitable devices, highlighting the chosen one.
    print_all_devices_with_selection(&suitable_devices, 0);

    // 6. Select the best device and get its queue information.
    let best_device_info = &suitable_devices[0];

    let queue_family_index_candidates = gather_queue_family_candidates(
        instance,
        surface_loader,
        surface_khr,
        best_device_info.device,
    );

    print_queue_family_info(
        instance,
        best_device_info.device,
        &queue_family_index_candidates,
    );

    let queue_family_indices = pick_best_queue_family_indices(&queue_family_index_candidates)
        .expect("Failed to pick queue families for a device that was already deemed suitable. This indicates a logic error.");

    print_selected_queue_families(&queue_family_indices);

    log::info!("Selected physical device: {}", best_device_info.device_name);

    (best_device_info.device, queue_family_indices)
}

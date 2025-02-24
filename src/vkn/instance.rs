use ash::{
    ext::debug_utils,
    vk::{self},
    Entry,
};
use std::{
    ffi::{CStr, CString},
    os::raw::{c_char, c_void},
};
use winit::{raw_window_handle::HasDisplayHandle, window::Window};

pub struct Instance {
    instance: ash::Instance,
    debug_utils: debug_utils::Instance,
    debug_utils_messenger: vk::DebugUtilsMessengerEXT,
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            self.debug_utils
                .destroy_debug_utils_messenger(self.debug_utils_messenger, None);
            self.instance.destroy_instance(None);
        }
    }
}

impl Instance {
    pub fn new(entry: &Entry, window: &Window, title: &str) -> Self {
        let (instance, debug_utils, debug_utils_messenger) =
            create_vulkan_instance(entry, window, title);
        Self {
            instance,
            debug_utils,
            debug_utils_messenger,
        }
    }

    pub fn as_raw(&self) -> &ash::Instance {
        &self.instance
    }
}

pub fn create_vulkan_instance(
    entry: &Entry,
    window: &Window,
    title: &str,
) -> (
    ash::Instance,
    debug_utils::Instance,
    vk::DebugUtilsMessengerEXT,
) {
    let app_name = CString::new(title).unwrap();
    let app_info = vk::ApplicationInfo::default()
        .application_name(app_name.as_c_str())
        .engine_name(c"No Engine")
        .application_version(vk::make_api_version(0, 0, 1, 0))
        .engine_version(vk::make_api_version(0, 0, 1, 0))
        .api_version(vk::make_api_version(0, 1, 3, 0));

    let mut extension_names =
        ash_window::enumerate_required_extensions(window.display_handle().unwrap().as_raw())
            .unwrap()
            .to_vec();
    extension_names.push(debug_utils::NAME.as_ptr());

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
        extension_names.push(ash::khr::get_physical_device_properties2::NAME.as_ptr());
    }

    let create_flags = if cfg!(any(target_os = "macos", target_os = "ios")) {
        vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
    } else {
        vk::InstanceCreateFlags::default()
    };

    let layer_names_raw: Vec<*const c_char> = {
        #[cfg(not(feature = "no_validation_layer"))]
        {
            {
                let layer_names = [c"VK_LAYER_KHRONOS_validation"];
                layer_names
                    .iter()
                    .map(|raw_name| raw_name.as_ptr())
                    .collect()
            }
        }
        #[cfg(feature = "no_validation_layer")]
        Vec::new()
    };

    let instance_create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .flags(create_flags)
        .enabled_layer_names(&layer_names_raw)
        .enabled_extension_names(&extension_names);

    let instance = unsafe { entry.create_instance(&instance_create_info, None).unwrap() };

    // vulkan debug report
    let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .flags(vk::DebugUtilsMessengerCreateFlagsEXT::empty())
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
        )
        .pfn_user_callback(Some(vulkan_debug_callback));

    let debug_utils = debug_utils::Instance::new(entry, &instance);
    let debug_utils_messenger = unsafe {
        debug_utils
            .create_debug_utils_messenger(&create_info, None)
            .unwrap()
    };

    (instance, debug_utils, debug_utils_messenger)
}

unsafe extern "system" fn vulkan_debug_callback(
    flag: vk::DebugUtilsMessageSeverityFlagsEXT,
    ty: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    use vk::DebugUtilsMessageSeverityFlagsEXT as Flag;

    let message = CStr::from_ptr((*p_callback_data).p_message);
    match flag {
        Flag::INFO => log::info!("[Validation] {:?} - {:?}", ty, message),
        Flag::WARNING => log::warn!("[Validation] {:?} - {:?}", ty, message),
        Flag::ERROR => log::error!("[Validation] {:?} - {:?}", ty, message),
        _ => log::error!("[Validation] Unexpected type met: {:?} - {:?}", ty, message),
    }
    vk::FALSE
}

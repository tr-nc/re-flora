use ash::{
    ext::debug_utils,
    vk::{self},
    Entry,
};
use std::{
    ffi::{CStr, CString},
    os::raw::{c_char, c_void},
    sync::Arc,
};
use winit::{raw_window_handle::HasDisplayHandle, window::Window};

struct InstanceInner {
    instance: ash::Instance,
    debug_utils: debug_utils::Instance,
    debug_utils_messenger: vk::DebugUtilsMessengerEXT,
}

impl Drop for InstanceInner {
    fn drop(&mut self) {
        unsafe {
            self.debug_utils
                .destroy_debug_utils_messenger(self.debug_utils_messenger, None);
            self.instance.destroy_instance(None);
        }
    }
}

#[derive(Clone)]
pub struct Instance(Arc<InstanceInner>);

impl std::ops::Deref for Instance {
    type Target = ash::Instance;
    fn deref(&self) -> &Self::Target {
        &self.0.instance
    }
}

impl Instance {
    pub fn new(entry: &Entry, window: &Window, title: &str) -> Self {
        let (instance, debug_utils, debug_utils_messenger) =
            create_vulkan_instance(entry, window, title);
        Self(Arc::new(InstanceInner {
            instance,
            debug_utils,
            debug_utils_messenger,
        }))
    }

    pub fn as_raw(&self) -> &ash::Instance {
        &self.0.instance
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

    let create_flags = vk::InstanceCreateFlags::default();

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
            vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
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

struct ValidationCallbackDesc {
    pretty_print: bool,
    log_level: log::Level,
}

const DEFAULT_VALIDATION_CALLBACK_DESC: ValidationCallbackDesc = ValidationCallbackDesc {
    pretty_print: true,
    log_level: log::Level::Error,
};

unsafe extern "system" fn vulkan_debug_callback(
    flag: vk::DebugUtilsMessageSeverityFlagsEXT,
    ty: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    use vk::DebugUtilsMessageSeverityFlagsEXT as Flag;

    let message_level = match flag {
        Flag::VERBOSE => log::Level::Debug,
        Flag::INFO => log::Level::Info,
        Flag::WARNING => log::Level::Warn,
        Flag::ERROR => log::Level::Error,
        _ => log::Level::Error, // Treat unknown flags as errors.
    };

    // ERROR is the lowest level in the enum
    if message_level > DEFAULT_VALIDATION_CALLBACK_DESC.log_level {
        return vk::FALSE;
    }

    let message = CStr::from_ptr((*p_callback_data).p_message).to_string_lossy();
    let header = format!("[Validation] {:?}", ty);

    if DEFAULT_VALIDATION_CALLBACK_DESC.pretty_print {
        let short_message = if let Some((msg, _)) = message.split_once(" (https://") {
            msg
        } else {
            &message
        };

        let formatted_parts = short_message
            .split('|')
            .map(|s| s.trim())
            .collect::<Vec<&str>>()
            .join("\n");

        let final_message = format!("\n* {}\n{}", header, formatted_parts);

        match flag {
            Flag::VERBOSE => log::debug!("{final_message}\n"),
            Flag::INFO => log::info!("{final_message}\n"),
            Flag::WARNING => log::warn!("{final_message}\n"),
            Flag::ERROR => log::error!("{final_message}\n"),
            _ => log::error!(
                "\n* {} with unknown severity {:?}\n{}\n",
                header,
                flag,
                formatted_parts
            ),
        }
    } else {
        // raw message output
        match flag {
            Flag::VERBOSE => log::debug!("{header} | {message}\n"),
            Flag::INFO => log::info!("{header} | {message}\n"),
            Flag::WARNING => log::warn!("{header} | {message}\n"),
            Flag::ERROR => log::error!("{header} | {message}\n"),
            _ => log::error!(
                "{} with unknown severity {:?} | {}\n",
                header,
                flag,
                message
            ),
        }
    }

    vk::FALSE
}

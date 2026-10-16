include!(concat!(env!("OUT_DIR"), "/precompiled_shaders.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    use spirv_reflect::{types::ReflectDescriptorType, ShaderModule};

    #[test]
    fn glass_resolve_screen_facts_do_not_consume_storage_image_slots() {
        let artifact = find_precompiled_shader("shader/tracer/glass_resolve.comp").unwrap();
        // Check both the layout's reflection and the code actually sent to Vulkan.
        for bytes in [artifact.reflection_spirv, artifact.optimized_spirv] {
            let shader = ShaderModule::load_u8_data(bytes).unwrap();
            let bindings = shader.enumerate_descriptor_bindings(None).unwrap();
            let storage: u32 = bindings
                .iter()
                .filter(|b| {
                    matches!(
                        b.descriptor_type,
                        ReflectDescriptorType::StorageImage
                            | ReflectDescriptorType::StorageTexelBuffer
                    )
                })
                .map(|b| b.count)
                .sum();
            assert!(
                storage <= 8,
                "glass resolve needs {storage} storage images; supported Metal device budget is 8"
            );
            for binding in [7, 8, 9, 10] {
                let input = bindings
                    .iter()
                    .find(|b| b.set == 0 && b.binding == binding)
                    .unwrap();
                assert_eq!(input.descriptor_type, ReflectDescriptorType::SampledImage,
                    "read-only screen input at binding {binding} must use unfiltered sampled-image loads");
            }
        }
    }
}

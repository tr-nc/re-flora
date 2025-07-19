use super::struct_layout::*;
use crate::{
    util::{full_path_from_relative, ShaderCompiler},
    vkn::{DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutBuilder, Device},
};
use anyhow::Result;
use ash::vk;
use shaderc::ShaderKind;
use spirv_reflect::{
    types::{
        ReflectDescriptorSet, ReflectDescriptorType, ReflectTypeDescriptionTraits, ReflectTypeFlags,
    },
    ShaderModule as ReflectShaderModule,
};
use std::{collections::HashMap, ffi::CString, fmt::Debug, sync::Arc};

/// Specifies a manual override for a vertex attribute's format.
///
/// This is used to tell the pipeline to use a more compact format (like `R8G8B8A8_UNORM`)
/// from the vertex buffer, even if the shader itself expects a wider type (like `vec4`).
/// The GPU's vertex fetch unit will handle the conversion.
/// See: https://github.com/ocornut/imgui/discussions/6049
#[derive(Debug, Clone, Copy)]
pub struct FormatOverride {
    /// The shader `location` of the attribute to override.
    pub location: u32,
    /// The `vk::Format` to use instead of the one from reflection.
    pub format: vk::Format,
}

/// Internal struct holding the actual Vulkan `ShaderModule` and reflection data.
struct ShaderModuleInner {
    device: Device,
    module_name: String,
    entry_point_name: CString,
    shader_module: vk::ShaderModule,
    reflect_shader_module: ReflectShaderModule,

    buffer_layouts: HashMap<String, BufferLayout>, // type_name - the buffer layout
}

impl Drop for ShaderModuleInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_shader_module(self.shader_module, None);
        }
    }
}

#[derive(Clone)]
pub struct ShaderModule(Arc<ShaderModuleInner>);

impl std::ops::Deref for ShaderModule {
    type Target = vk::ShaderModule;
    fn deref(&self) -> &Self::Target {
        &self.0.shader_module
    }
}

impl Debug for ShaderModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let descriptor_bindings = self
            .0
            .reflect_shader_module
            .enumerate_descriptor_bindings(None)
            .unwrap_or_else(|_| Vec::new()); // Handle potential error gracefully

        f.debug_struct("ShaderModule")
            .field("module_name", &self.0.module_name)
            .field("bindings_count", &descriptor_bindings.len())
            .field("bindings", &descriptor_bindings)
            .finish()
    }
}

impl ShaderModule {
    /// Create a new shader module from GLSL code, reflect it, and cache relevant layout metadata.
    ///
    /// * `device` - The device to create the shader module on.
    /// * `compiler` - The shader compiler to use.
    /// * `file_path` - The relative path to the GLSL file, from the project root.
    /// * `entry_point_name` - The name of the entry point function in the shader.
    pub fn from_glsl(
        device: &Device,
        compiler: &ShaderCompiler,
        file_path: &str,
        entry_point_name: &str,
    ) -> Result<Self, String> {
        let module_name = file_path.split('/').last().unwrap().to_string();
        let full_path = full_path_from_relative(file_path);
        let code = read_code_from_path(&full_path)?;
        let shader_kind = predict_shader_kind(file_path).map_err(|e| e.to_string())?;

        Self::from_glsl_code(
            device,
            &module_name,
            &code,
            &full_path,
            entry_point_name,
            compiler,
            shader_kind,
        )
    }

    pub fn get_buffer_layout(&self, name: &str) -> Result<&BufferLayout, String> {
        self.0
            .buffer_layouts
            .get(name)
            .ok_or_else(|| format!("Buffer layout not found for name: {}", name))
    }

    /// Retrieve the workgroup size (for compute shaders).
    pub fn get_workgroup_size(&self) -> Result<[u32; 3], String> {
        let entry_points = self
            .0
            .reflect_shader_module
            .enumerate_entry_points()
            .unwrap();

        if entry_points.len() != 1 {
            return Err("Multiple entry points found".to_string());
        }

        let entry_point = entry_points.first().unwrap();
        let local_size = entry_point.local_size;
        Ok([local_size.x, local_size.y, local_size.z])
    }

    /// Returns the Vulkan stage flags for this shader.
    pub fn get_stage(&self) -> vk::ShaderStageFlags {
        vk::ShaderStageFlags::from_raw(self.0.reflect_shader_module.get_shader_stage().bits())
    }

    /// Returns the underlying Vulkan `vk::ShaderModule`.
    pub fn get_shader_module(&self) -> vk::ShaderModule {
        self.0.shader_module
    }

    /// Convenience for creating a stage create info (for pipeline creation).
    pub fn get_shader_stage_create_info(&self) -> vk::PipelineShaderStageCreateInfo {
        vk::PipelineShaderStageCreateInfo::default()
            .stage(self.get_stage())
            .module(self.get_shader_module())
            .name(&self.0.entry_point_name)
    }

    pub fn get_vertex_input_state(
        &self,
        format_overrides: &[FormatOverride],
        instance_rate_starting_location: Option<u32>,
    ) -> Result<(
        Vec<vk::VertexInputBindingDescription>,
        Vec<vk::VertexInputAttributeDescription>,
    )> {
        if self.get_stage() != vk::ShaderStageFlags::VERTEX {
            return Err(anyhow::anyhow!(
                "Shader module is not a vertex shader, stage: {:?}",
                self.get_stage()
            )
            .into());
        }

        let mut input_vars = self
            .0
            .reflect_shader_module
            .enumerate_input_variables(None)
            .expect("Failed to enumerate input variables from shader");

        // otherwise the order of the attributes is not guaranteed, leading to incorrect offset calculation
        input_vars.sort_by_key(|var| var.location);

        let mut attribute_descriptions = Vec::with_capacity(input_vars.len());
        // Use an array or a small Vec to track offsets for each binding.
        // Assuming max 2 bindings for simplicity.
        let mut offsets = [0u32; 2];

        let vert_rate_stride;
        let mut inst_rate_stride = None;

        let mut binding_index = 0;

        for var in &input_vars {
            let loc = var.location;

            if var
                .decoration_flags
                .contains(spirv_reflect::types::ReflectDecorationFlags::BUILT_IN)
            {
                continue;
            }

            let reflected_format = reflect_format_to_vk(var.format)?;
            let final_format = format_overrides
                .iter()
                .find(|ov| ov.location == loc)
                .map_or(reflected_format, |ov| ov.format);

            // Check if we've crossed into the instance-rate attributes and update the binding index
            if let Some(start_loc) = instance_rate_starting_location {
                if loc >= start_loc {
                    binding_index = 1;
                }
            }

            let description = vk::VertexInputAttributeDescription::default()
                .binding(binding_index as u32)
                .location(loc)
                .format(final_format)
                // Use the offset for the CURRENT binding
                .offset(offsets[binding_index]);

            attribute_descriptions.push(description);

            // Increment the offset for the CURRENT binding
            offsets[binding_index] += format_to_size_in_bytes(final_format);
        }

        // Final strides are just the total accumulated offsets for each binding
        vert_rate_stride = offsets[0];
        if instance_rate_starting_location.is_some() {
            inst_rate_stride = Some(offsets[1]);
        }

        if attribute_descriptions.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }

        let mut binding_description = vec![vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(vert_rate_stride)
            .input_rate(vk::VertexInputRate::VERTEX)];
        if let Some(inst_rate_stride) = inst_rate_stride {
            binding_description.push(
                vk::VertexInputBindingDescription::default()
                    .binding(1)
                    .stride(inst_rate_stride)
                    .input_rate(vk::VertexInputRate::INSTANCE),
            );
        }

        return Ok((binding_description, attribute_descriptions));

        fn format_to_size_in_bytes(format: vk::Format) -> u32 {
            match format {
                // 8-bit formats
                vk::Format::R8_UNORM
                | vk::Format::R8_SNORM
                | vk::Format::R8_UINT
                | vk::Format::R8_SINT => 1,
                vk::Format::R8G8_UNORM
                | vk::Format::R8G8_SNORM
                | vk::Format::R8G8_UINT
                | vk::Format::R8G8_SINT => 2,
                vk::Format::R8G8B8_UNORM
                | vk::Format::R8G8B8_SNORM
                | vk::Format::R8G8B8_UINT
                | vk::Format::R8G8B8_SINT => 3,
                vk::Format::R8G8B8A8_UNORM
                | vk::Format::R8G8B8A8_SNORM
                | vk::Format::R8G8B8A8_UINT
                | vk::Format::R8G8B8A8_SINT
                | vk::Format::R8G8B8A8_SRGB => 4,

                // 16-bit formats
                vk::Format::R16_UNORM
                | vk::Format::R16_SNORM
                | vk::Format::R16_UINT
                | vk::Format::R16_SINT
                | vk::Format::R16_SFLOAT => 2,
                vk::Format::R16G16_UNORM
                | vk::Format::R16G16_SNORM
                | vk::Format::R16G16_UINT
                | vk::Format::R16G16_SINT
                | vk::Format::R16G16_SFLOAT => 4,
                vk::Format::R16G16B16_UNORM
                | vk::Format::R16G16B16_SNORM
                | vk::Format::R16G16B16_UINT
                | vk::Format::R16G16B16_SINT
                | vk::Format::R16G16B16_SFLOAT => 6,
                vk::Format::R16G16B16A16_UNORM
                | vk::Format::R16G16B16A16_SNORM
                | vk::Format::R16G16B16A16_UINT
                | vk::Format::R16G16B16A16_SINT
                | vk::Format::R16G16B16A16_SFLOAT => 8,

                // 32-bit formats
                vk::Format::R32_UINT | vk::Format::R32_SINT | vk::Format::R32_SFLOAT => 4,
                vk::Format::R32G32_UINT | vk::Format::R32G32_SINT | vk::Format::R32G32_SFLOAT => 8,
                vk::Format::R32G32B32_UINT
                | vk::Format::R32G32B32_SINT
                | vk::Format::R32G32B32_SFLOAT => 12,
                vk::Format::R32G32B32A32_UINT
                | vk::Format::R32G32B32A32_SINT
                | vk::Format::R32G32B32A32_SFLOAT => 16,

                // 64-bit formats (double precision)
                vk::Format::R64_UINT | vk::Format::R64_SINT | vk::Format::R64_SFLOAT => 8,
                vk::Format::R64G64_UINT | vk::Format::R64G64_SINT | vk::Format::R64G64_SFLOAT => 16,
                vk::Format::R64G64B64_UINT
                | vk::Format::R64G64B64_SINT
                | vk::Format::R64G64B64_SFLOAT => 24,
                vk::Format::R64G64B64A64_UINT
                | vk::Format::R64G64B64A64_SINT
                | vk::Format::R64G64B64A64_SFLOAT => 32,

                // Packed formats
                vk::Format::A2B10G10R10_UNORM_PACK32 | vk::Format::A2B10G10R10_UINT_PACK32 => 4,

                _ => panic!(
                    "Unsupported vertex format for size calculation: {:?}",
                    format
                ),
            }
        }

        fn reflect_format_to_vk(fmt: spirv_reflect::types::ReflectFormat) -> Result<vk::Format> {
            use spirv_reflect::types::ReflectFormat as RF;
            match fmt {
                RF::Undefined => Err(anyhow::anyhow!("Cannot reflect an undefined format.")),
                RF::R32_UINT => Ok(vk::Format::R32_UINT),
                RF::R32_SINT => Ok(vk::Format::R32_SINT),
                RF::R32_SFLOAT => Ok(vk::Format::R32_SFLOAT),
                RF::R32G32_UINT => Ok(vk::Format::R32G32_UINT),
                RF::R32G32_SINT => Ok(vk::Format::R32G32_SINT),
                RF::R32G32_SFLOAT => Ok(vk::Format::R32G32_SFLOAT),
                RF::R32G32B32_UINT => Ok(vk::Format::R32G32B32_UINT),
                RF::R32G32B32_SINT => Ok(vk::Format::R32G32B32_SINT),
                RF::R32G32B32_SFLOAT => Ok(vk::Format::R32G32B32_SFLOAT),
                RF::R32G32B32A32_UINT => Ok(vk::Format::R32G32B32A32_UINT),
                RF::R32G32B32A32_SINT => Ok(vk::Format::R32G32B32A32_SINT),
                RF::R32G32B32A32_SFLOAT => Ok(vk::Format::R32G32B32A32_SFLOAT),
            }
        }
    }

    fn from_glsl_code(
        device: &Device,
        module_name: &str,
        code: &str,
        full_path_to_shader_file: &str,
        entry_point_name: &str,
        compiler: &ShaderCompiler,
        shader_kind: ShaderKind,
    ) -> Result<Self, String> {
        let reflect_sm = create_reflect_shader_module(
            code,
            shader_kind,
            entry_point_name,
            full_path_to_shader_file,
            compiler,
        )?;

        let sm = create_shader_module(
            device,
            code,
            shader_kind,
            entry_point_name,
            full_path_to_shader_file,
            compiler,
        )?;

        let buffer_layouts = extract_buffer_layouts(&reflect_sm).map_err(|e| e.to_string())?;

        Ok(Self(Arc::new(ShaderModuleInner {
            device: device.clone(),
            module_name: module_name.to_string(),
            entry_point_name: CString::new(entry_point_name).unwrap(),
            shader_module: sm,
            reflect_shader_module: reflect_sm,
            buffer_layouts,
        })))
    }

    fn get_reflect_descriptor_sets(&self) -> HashMap<u32, ReflectDescriptorSet> {
        let descriptor_sets = self
            .0
            .reflect_shader_module
            .enumerate_descriptor_sets(None)
            .expect("Failed to enumerate descriptor sets");

        if descriptor_sets.is_empty() {
            return HashMap::new();
        }

        let mut sets: HashMap<u32, ReflectDescriptorSet> = HashMap::new();

        for set in descriptor_sets {
            let set_no = set.set;
            sets.insert(set_no, set);
        }

        sets
    }

    pub fn get_push_constant_ranges(&self) -> HashMap<u32, vk::PushConstantRange> {
        let push_constant_blocks = self
            .0
            .reflect_shader_module
            .enumerate_push_constant_blocks(None)
            .expect("Failed to enumerate push constant blocks");

        if push_constant_blocks.is_empty() {
            return HashMap::new();
        }

        let mut ranges = HashMap::new();
        for block in push_constant_blocks {
            ranges.insert(
                block.offset,
                vk::PushConstantRange {
                    stage_flags: self.get_stage(),
                    offset: block.offset,
                    size: block.size,
                },
            );
        }
        ranges
    }

    fn get_descriptor_set_bindings(
        &self,
        reflect_descriptor_set: &ReflectDescriptorSet,
    ) -> HashMap<u32, DescriptorSetLayoutBinding> {
        let mut bindings = HashMap::new();
        for binding in &reflect_descriptor_set.bindings {
            bindings.insert(
                binding.binding,
                DescriptorSetLayoutBinding {
                    no: binding.binding,
                    name: binding.name.clone(),
                    descriptor_type: reflect_descriptor_type_to_descriptor_type(
                        binding.descriptor_type,
                    ),
                    descriptor_count: binding.count,
                    stage_flags: self.get_stage(),
                },
            );
        }
        bindings
    }

    pub fn get_descriptor_sets_bindings(
        &self,
    ) -> HashMap<u32, HashMap<u32, DescriptorSetLayoutBinding>> {
        let refl_descriptor_sets = self.get_reflect_descriptor_sets();
        let mut bindings = HashMap::new();
        for refl_ds in refl_descriptor_sets {
            bindings.insert(refl_ds.0, self.get_descriptor_set_bindings(&refl_ds.1));
        }
        return bindings;
    }

    pub fn get_descriptor_set_layouts(&self) -> HashMap<u32, DescriptorSetLayout> {
        let bindings = self.get_descriptor_sets_bindings();
        let mut layouts = HashMap::new();
        for (set_no, bindings) in bindings {
            let mut builder = DescriptorSetLayoutBuilder::new();
            let binding_vec: Vec<DescriptorSetLayoutBinding> = bindings.values().cloned().collect();
            builder.add_bindings(&binding_vec);
            layouts.insert(set_no, builder.build(&self.0.device).unwrap());
        }
        layouts
    }
}

/// With zero optimization, no unused bindings will be removed, and the names of the
/// variables will be preserved accurately
fn create_reflect_shader_module(
    code: &str,
    shader_kind: ShaderKind,
    entry_point_name: &str,
    full_path_to_shader_file: &str,
    compiler: &ShaderCompiler,
) -> Result<ReflectShaderModule, String> {
    let shader_byte_code_u8_zero_opti = compiler
        .compile_to_bytecode(
            code,
            shader_kind,
            entry_point_name,
            full_path_to_shader_file,
            shaderc::OptimizationLevel::Zero,
        )
        .map_err(|e| e.to_string())?;
    ReflectShaderModule::load_u8_data(&shader_byte_code_u8_zero_opti).map_err(|e| e.to_string())
}

/// Compile the actual shader module with full optimization.
fn create_shader_module(
    device: &Device,
    code: &str,
    shader_kind: ShaderKind,
    entry_point_name: &str,
    full_path_to_shader_file: &str,
    compiler: &ShaderCompiler,
) -> Result<vk::ShaderModule, String> {
    let shader_byte_code_u8_full_opti = compiler
        .compile_to_bytecode(
            code,
            shader_kind,
            entry_point_name,
            full_path_to_shader_file,
            shaderc::OptimizationLevel::Performance,
        )
        .map_err(|e| e.to_string())?;

    bytecode_to_shader_module(device, &shader_byte_code_u8_full_opti)
}

fn read_code_from_path(full_shader_path: &str) -> Result<String, String> {
    std::fs::read_to_string(full_shader_path).map_err(|e| e.to_string())
}

/// A simple extension-based guess of the shader kind (vert, frag, comp).
fn predict_shader_kind(file_path: &str) -> Result<shaderc::ShaderKind, String> {
    match file_path.split('.').last() {
        Some("vert") => Ok(shaderc::ShaderKind::Vertex),
        Some("frag") => Ok(shaderc::ShaderKind::Fragment),
        Some("comp") => Ok(shaderc::ShaderKind::Compute),
        _ => Err(format!("Unknown shader extension: {}", file_path)),
    }
}

/// Convert our SPIR-V bytecode (u8 Vec) into a Vulkan `ShaderModule`.
fn bytecode_to_shader_module(
    device: &Device,
    shader_byte_code: &[u8],
) -> Result<vk::ShaderModule, String> {
    let shader_byte_code_u32 = u8_to_u32(shader_byte_code);
    let shader_module_create_info =
        vk::ShaderModuleCreateInfo::default().code(&shader_byte_code_u32);
    unsafe {
        device
            .create_shader_module(&shader_module_create_info, None)
            .map_err(|e| e.to_string())
    }
}

/// Converts a byte slice (SPIR-V) into a `Vec<u32>` for Vulkan.
fn u8_to_u32(byte_code: &[u8]) -> Vec<u32> {
    byte_code
        .chunks_exact(4)
        .map(|chunk| {
            let mut bytes = [0u8; 4];
            bytes.copy_from_slice(chunk);
            u32::from_ne_bytes(bytes)
        })
        .collect()
}

fn extract_buffer_layouts(
    reflect_module: &ReflectShaderModule,
) -> Result<HashMap<String, BufferLayout>, String> {
    let bindings = match reflect_module.enumerate_descriptor_bindings(None) {
        Ok(binding) => binding,
        Err(_) => return Err("Failed to enumerate descriptor bindings".to_string()),
    };

    let mut result = HashMap::new();

    for binding in bindings {
        if !is_buffer_type(binding.descriptor_type) {
            continue;
        }

        let type_description = &binding.type_description.unwrap();
        let ty = type_description.type_name.clone();
        let name = binding.name.clone();
        let descriptor_type = binding.descriptor_type;
        let block = binding.block;
        let members = parse_members_recursive(&block.members);

        let root_member = StructMemberLayout {
            name,
            ty: ty.clone(),
            name_member_table: members,
        };

        let layout = BufferLayout {
            root_member,
            descriptor_type,
        };

        result.insert(ty, layout);
    }

    return Ok(result);

    fn is_buffer_type(ty: ReflectDescriptorType) -> bool {
        ty == ReflectDescriptorType::UniformBuffer || ty == ReflectDescriptorType::StorageBuffer
    }

    fn parse_members_recursive(
        reflect_members: &[spirv_reflect::types::ReflectBlockVariable],
    ) -> HashMap<String, MemberLayout> {
        let mut result = HashMap::new();
        for (_, reflect_member) in reflect_members.iter().enumerate() {
            let member_name = reflect_member.name.clone();
            let type_description = reflect_member.type_description.as_ref().unwrap();
            let type_flags = &type_description.type_flags;
            let member_type = get_general_member_type(type_flags);

            let member: MemberLayout = match member_type {
                GeneralMemberType::Array | GeneralMemberType::Plain => {
                    let size = reflect_member.size as u64;
                    // notice: u64 is not supported yet in the reflect lib, but we use u64 in our code for the best extensibility
                    let offset = reflect_member.offset as u64;
                    let padded_size = reflect_member.padded_size as u64;

                    let ty =
                        get_plain_member_type(type_flags, &type_description.traits, size).unwrap();
                    MemberLayout::Plain(PlainMemberLayout {
                        name: member_name.clone(),
                        ty,
                        offset,
                        size,
                        padded_size,
                    })
                }
                GeneralMemberType::Struct => {
                    let ty = type_description.type_name.clone();
                    let members = parse_members_recursive(&reflect_member.members);
                    MemberLayout::Struct(StructMemberLayout {
                        name: member_name.clone(),
                        ty,
                        name_member_table: members,
                    })
                }
            };
            result.insert(member_name.clone(), member);
        }
        return result;

        fn get_general_member_type(type_flags: &ReflectTypeFlags) -> GeneralMemberType {
            if type_flags.contains(ReflectTypeFlags::STRUCT) {
                GeneralMemberType::Struct
            } else {
                GeneralMemberType::Plain
                // notice: Array type is not supported yet, and is counted as plain type
            }
        }

        fn get_plain_member_type(
            type_flags: &ReflectTypeFlags,
            traits: &ReflectTypeDescriptionTraits,
            size: u64,
        ) -> Result<PlainMemberType, String> {
            assert!(
                get_general_member_type(type_flags) == GeneralMemberType::Plain,
                "Expected plain member type",
            );

            let numeric = &traits.numeric;

            if type_flags.contains(ReflectTypeFlags::ARRAY) {
                return Ok(PlainMemberType::Array);
            }

            // Matrices
            if type_flags.contains(ReflectTypeFlags::MATRIX) {
                let cols = numeric.matrix.column_count;
                let rows = numeric.matrix.row_count;
                return match (rows, cols) {
                    (4, 4) => Ok(PlainMemberType::Mat4),
                    (3, 3) => Ok(PlainMemberType::Mat3),
                    (2, 2) => Ok(PlainMemberType::Mat2),
                    (4, 3) => Ok(PlainMemberType::Mat3x4),
                    _ => Err(format!("Unsupported matrix size: {}x{}", rows, cols)),
                };
            }

            // Vectors
            if type_flags.contains(ReflectTypeFlags::VECTOR) {
                let comp_count = numeric.vector.component_count;
                // Distinguish float-based vs int-based vs uint-based
                let is_float = type_flags.contains(ReflectTypeFlags::FLOAT);
                let is_int = type_flags.contains(ReflectTypeFlags::INT);
                let signedness = numeric.scalar.signedness;

                if is_float {
                    return match comp_count {
                        2 => Ok(PlainMemberType::Vec2),
                        3 => Ok(PlainMemberType::Vec3),
                        4 => Ok(PlainMemberType::Vec4),
                        _ => Err("Unsupported vector size".to_string()),
                    };
                } else if is_int {
                    // signedness == 1 => ivec..., else uvec...
                    if signedness == 1 {
                        return match comp_count {
                            2 => Ok(PlainMemberType::IVec2),
                            3 => Ok(PlainMemberType::IVec3),
                            4 => Ok(PlainMemberType::IVec4),
                            _ => Err("Unsupported vector size".to_string()),
                        };
                    } else {
                        return match comp_count {
                            2 => Ok(PlainMemberType::UVec2),
                            3 => Ok(PlainMemberType::UVec3),
                            4 => Ok(PlainMemberType::UVec4),
                            _ => Err("Unsupported vector size".to_string()),
                        };
                    }
                }
            }

            // Scalars
            if type_flags.contains(ReflectTypeFlags::FLOAT) {
                return Ok(PlainMemberType::Float);
            }

            if type_flags.contains(ReflectTypeFlags::INT) {
                // "bool" in GLSL is 32-bit in SPIR-V, typically stored as int.
                let signed = numeric.scalar.signedness;
                if size == 4 {
                    return if signed == 1 {
                        Ok(PlainMemberType::Int)
                    } else {
                        Ok(PlainMemberType::UInt)
                    };
                }
                if size == 8 {
                    return if signed == 1 {
                        Ok(PlainMemberType::Int64)
                    } else {
                        Ok(PlainMemberType::UInt64)
                    };
                }
            }

            return Err("Unsupported plain member type".to_string());
        }
    }
}

fn reflect_descriptor_type_to_descriptor_type(
    reflect_type: ReflectDescriptorType,
) -> vk::DescriptorType {
    match reflect_type {
        ReflectDescriptorType::Sampler => vk::DescriptorType::SAMPLER,
        ReflectDescriptorType::CombinedImageSampler => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        ReflectDescriptorType::SampledImage => vk::DescriptorType::SAMPLED_IMAGE,
        ReflectDescriptorType::StorageImage => vk::DescriptorType::STORAGE_IMAGE,
        ReflectDescriptorType::UniformTexelBuffer => vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
        ReflectDescriptorType::StorageTexelBuffer => vk::DescriptorType::STORAGE_TEXEL_BUFFER,
        ReflectDescriptorType::UniformBuffer => vk::DescriptorType::UNIFORM_BUFFER,
        ReflectDescriptorType::StorageBuffer => vk::DescriptorType::STORAGE_BUFFER,
        ReflectDescriptorType::UniformBufferDynamic => vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
        ReflectDescriptorType::StorageBufferDynamic => vk::DescriptorType::STORAGE_BUFFER_DYNAMIC,
        ReflectDescriptorType::InputAttachment => vk::DescriptorType::INPUT_ATTACHMENT,
        ReflectDescriptorType::AccelerationStructureKHR => {
            vk::DescriptorType::ACCELERATION_STRUCTURE_KHR
        }
        _ => panic!("Unsupported descriptor type in reflection."),
    }
}

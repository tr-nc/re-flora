use std::collections::HashMap;

use super::struct_layout::StructLayout;

/// Builder for creating properly aligned buffer data according to a shader's struct layout
pub struct BufferBuilder<'a> {
    layout: &'a StructLayout,
    values: HashMap<String, Vec<u8>>,
}

impl<'a> BufferBuilder<'a> {
    /// Create a new BufferBuilder from a reflected struct layout
    pub fn with_layout(layout: &'a StructLayout) -> Self {
        Self {
            layout,
            values: HashMap::new(),
        }
    }

    /// Helper method for setting values with type checking
    fn set_value_with_bytes(mut self, name: &str, bytes: Vec<u8>, expected_type: &str) -> Self {
        if let Some(member) = self.layout.get_member(name) {
            if member.type_name == expected_type {
                self.values.insert(name.to_string(), bytes);
            } else {
                log::warn!(
                    "Member {} is not a {}, it's a {}",
                    name,
                    expected_type,
                    member.type_name
                );
            }
        } else {
            log::warn!("Member {} not found in layout", name);
        }
        self
    }

    /// Set a float value for the named member
    pub fn set_float(self, name: &str, value: f32) -> Self {
        self.set_value_with_bytes(name, value.to_ne_bytes().to_vec(), "float")
    }

    /// Set a uint value for the named member
    pub fn set_uint(self, name: &str, value: u32) -> Self {
        self.set_value_with_bytes(name, value.to_ne_bytes().to_vec(), "uint")
    }

    /// Set an int value for the named member
    pub fn set_int(self, name: &str, value: i32) -> Self {
        self.set_value_with_bytes(name, value.to_ne_bytes().to_vec(), "int")
    }

    /// Set a vec2 value for the named member
    pub fn set_vec2(self, name: &str, value: [f32; 2]) -> Self {
        let mut bytes = Vec::with_capacity(8);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "vec2")
    }

    /// Set a vec3 value for the named member
    pub fn set_vec3(self, name: &str, value: [f32; 3]) -> Self {
        let mut bytes = Vec::with_capacity(12);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "vec3")
    }

    /// Set a vec4 value for the named member
    pub fn set_vec4(self, name: &str, value: [f32; 4]) -> Self {
        let mut bytes = Vec::with_capacity(16);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "vec4")
    }

    /// Set an ivec2 value for the named member
    pub fn set_ivec2(self, name: &str, value: [i32; 2]) -> Self {
        let mut bytes = Vec::with_capacity(8);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "ivec2")
    }

    /// Set an ivec3 value for the named member
    pub fn set_ivec3(self, name: &str, value: [i32; 3]) -> Self {
        let mut bytes = Vec::with_capacity(12);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "ivec3")
    }

    /// Set an ivec4 value for the named member
    pub fn set_ivec4(self, name: &str, value: [i32; 4]) -> Self {
        let mut bytes = Vec::with_capacity(16);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "ivec4")
    }

    /// Set a uvec2 value for the named member
    pub fn set_uvec2(self, name: &str, value: [u32; 2]) -> Self {
        let mut bytes = Vec::with_capacity(8);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "uvec2")
    }

    /// Set a uvec3 value for the named member
    pub fn set_uvec3(self, name: &str, value: [u32; 3]) -> Self {
        let mut bytes = Vec::with_capacity(12);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "uvec3")
    }

    /// Set a uvec4 value for the named member
    pub fn set_uvec4(self, name: &str, value: [u32; 4]) -> Self {
        let mut bytes = Vec::with_capacity(16);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "uvec4")
    }

    /// Set a mat2 value for the named member
    pub fn set_mat2(self, name: &str, value: [[f32; 2]; 2]) -> Self {
        let mut bytes = Vec::with_capacity(16);
        for row in &value {
            for v in row {
                bytes.extend_from_slice(&v.to_ne_bytes());
            }
        }
        self.set_value_with_bytes(name, bytes, "mat2")
    }

    /// Set a mat3 value for the named member
    pub fn set_mat3(self, name: &str, value: [[f32; 3]; 3]) -> Self {
        let mut bytes = Vec::with_capacity(36);
        for row in &value {
            for v in row {
                bytes.extend_from_slice(&v.to_ne_bytes());
            }
        }
        self.set_value_with_bytes(name, bytes, "mat3")
    }

    /// Set a mat4 value for the named member
    pub fn set_mat4(self, name: &str, value: [[f32; 4]; 4]) -> Self {
        let mut bytes = Vec::with_capacity(64);
        for row in &value {
            for v in row {
                bytes.extend_from_slice(&v.to_ne_bytes());
            }
        }
        self.set_value_with_bytes(name, bytes, "mat4")
    }

    /// Build the final buffer data with proper alignment and padding
    pub fn build(self) -> Vec<u8> {
        let total_size = self.layout.total_size as usize;
        let mut buffer = vec![0u8; total_size];

        // Verify all members are set
        for (name, _) in &self.layout.members {
            if !self.values.contains_key(name) {
                log::warn!("Member {} not set in buffer", name);
            }
        }

        // Fill the buffer with values at the correct offsets
        for (name, bytes) in self.values {
            if let Some(member) = self.layout.get_member(&name) {
                let offset = member.offset as usize;
                let size = bytes.len();

                // Ensure we don't write past the padded size
                if size > member.padded_size as usize {
                    log::warn!(
                        "Member {} has more data ({} bytes) than padded size ({} bytes)",
                        name,
                        size,
                        member.padded_size
                    );
                    buffer[offset..offset + member.padded_size as usize]
                        .copy_from_slice(&bytes[..member.padded_size as usize]);
                } else {
                    buffer[offset..offset + size].copy_from_slice(&bytes);
                }
            }
        }

        buffer
    }
}

use log::warn;
use std::collections::HashMap;
use std::convert::TryInto;

use super::struct_layout::StructLayout;

/// Builder for creating properly aligned buffer data according to a shader's struct layout.
///
/// This builder can be used in two modes:
/// - **Write Mode:** Use the various `set_*` methods to build up values and then create the
///   raw buffer (via `to_raw_data()`) to be sent to the GPU.
/// - **Read Mode:** After fetching the raw data from a GPU buffer (using, e.g., `fetch_raw()`),
///   call `set_raw` and use the getter methods (e.g. `get_vec4`) to query individual field values.
pub struct BufferBuilder<'a> {
    layout: &'a StructLayout,
    /// Map of member names to their corresponding bytes (write mode)
    values: HashMap<String, Vec<u8>>,
    /// Raw buffer data, used in read mode
    raw_data: Option<Vec<u8>>,
}

impl<'a> BufferBuilder<'a> {
    /// Create a new BufferBuilder from a given struct layout.
    pub fn from_layout(layout: &'a StructLayout) -> Self {
        Self {
            layout,
            values: HashMap::new(),
            raw_data: None,
        }
    }

    /// Internal helper to set a value if the member exists and has the expected type.
    fn set_value_with_bytes(mut self, name: &str, bytes: Vec<u8>, expected_type: &str) -> Self {
        if let Some(member) = self.layout.get_member(name) {
            if member.type_name == expected_type {
                self.values.insert(name.to_string(), bytes);
            } else {
                warn!(
                    "Member {} is not a {}, it's a {}",
                    name, expected_type, member.type_name
                );
            }
        } else {
            warn!("Member {} not found in layout", name);
        }
        self
    }

    ////////////////////////////////////////////////////////////////////////////////
    // Write Mode: Setter Methods
    ////////////////////////////////////////////////////////////////////////////////

    pub fn set_float(self, name: &str, value: f32) -> Self {
        self.set_value_with_bytes(name, value.to_ne_bytes().to_vec(), "float")
    }

    pub fn set_uint(self, name: &str, value: u32) -> Self {
        self.set_value_with_bytes(name, value.to_ne_bytes().to_vec(), "uint")
    }

    pub fn set_int(self, name: &str, value: i32) -> Self {
        self.set_value_with_bytes(name, value.to_ne_bytes().to_vec(), "int")
    }

    pub fn set_vec2(self, name: &str, value: [f32; 2]) -> Self {
        let mut bytes = Vec::with_capacity(8);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "vec2")
    }

    pub fn set_vec3(self, name: &str, value: [f32; 3]) -> Self {
        let mut bytes = Vec::with_capacity(12);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "vec3")
    }

    pub fn set_vec4(self, name: &str, value: [f32; 4]) -> Self {
        let mut bytes = Vec::with_capacity(16);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "vec4")
    }

    pub fn set_ivec2(self, name: &str, value: [i32; 2]) -> Self {
        let mut bytes = Vec::with_capacity(8);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "ivec2")
    }

    pub fn set_ivec3(self, name: &str, value: [i32; 3]) -> Self {
        let mut bytes = Vec::with_capacity(12);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "ivec3")
    }

    pub fn set_ivec4(self, name: &str, value: [i32; 4]) -> Self {
        let mut bytes = Vec::with_capacity(16);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "ivec4")
    }

    pub fn set_uvec2(self, name: &str, value: [u32; 2]) -> Self {
        let mut bytes = Vec::with_capacity(8);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "uvec2")
    }

    pub fn set_uvec3(self, name: &str, value: [u32; 3]) -> Self {
        let mut bytes = Vec::with_capacity(12);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "uvec3")
    }

    pub fn set_uvec4(self, name: &str, value: [u32; 4]) -> Self {
        let mut bytes = Vec::with_capacity(16);
        for v in &value {
            bytes.extend_from_slice(&v.to_ne_bytes());
        }
        self.set_value_with_bytes(name, bytes, "uvec4")
    }

    pub fn set_mat2(self, name: &str, value: [[f32; 2]; 2]) -> Self {
        let mut bytes = Vec::with_capacity(16);
        for row in &value {
            for v in row {
                bytes.extend_from_slice(&v.to_ne_bytes());
            }
        }
        self.set_value_with_bytes(name, bytes, "mat2")
    }

    pub fn set_mat3(self, name: &str, value: [[f32; 3]; 3]) -> Self {
        let mut bytes = Vec::with_capacity(36);
        for row in &value {
            for v in row {
                bytes.extend_from_slice(&v.to_ne_bytes());
            }
        }
        self.set_value_with_bytes(name, bytes, "mat3")
    }

    pub fn set_mat4(self, name: &str, value: [[f32; 4]; 4]) -> Self {
        let mut bytes = Vec::with_capacity(64);
        for row in &value {
            for v in row {
                bytes.extend_from_slice(&v.to_ne_bytes());
            }
        }
        self.set_value_with_bytes(name, bytes, "mat4")
    }

    /// Consumes the builder and returns the raw data buffer that can be uploaded to GPU.
    pub fn to_raw_data(self) -> Vec<u8> {
        let total_size = self.layout.total_size as usize;
        let mut buffer = vec![0u8; total_size];

        // Warn if any member is missing.
        for (name, _) in &self.layout.members {
            if !self.values.contains_key(name) {
                warn!("Member {} not set in buffer", name);
            }
        }

        // Copy each member's bytes into the correct offset.
        for (name, bytes) in self.values {
            if let Some(member) = self.layout.get_member(&name) {
                let offset = member.offset as usize;
                let size = bytes.len();

                if size > member.padded_size as usize {
                    warn!(
                        "Member {} has more data ({} bytes) than padded size ({} bytes)",
                        name, size, member.padded_size
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

    ////////////////////////////////////////////////////////////////////////////////
    // Read Mode: Setter and Getter Methods
    ////////////////////////////////////////////////////////////////////////////////

    /// Set the raw buffer data obtained from the GPU (e.g., via fetch_raw).
    ///
    /// This switches the builder to “read mode” so that you can query individual field values.
    pub fn set_raw(mut self, data: Vec<u8>) -> Self {
        if data.len() != self.layout.total_size as usize {
            warn!(
                "Provided raw data size {} does not match expected total size {}",
                data.len(),
                self.layout.total_size
            );
        }
        self.raw_data = Some(data);
        self
    }

    /// Getter for a `float` (4 bytes).
    pub fn get_float(&self, name: &str) -> Option<f32> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "float" {
            warn!(
                "Member {} is not of type float, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 4 > raw_data.len() {
            warn!("Raw data is too small for float at member {}", name);
            return None;
        }
        let bytes: [u8; 4] = raw_data[offset..offset + 4].try_into().ok()?;
        Some(f32::from_ne_bytes(bytes))
    }

    /// Getter for a `uint` (u32, 4 bytes).
    pub fn get_uint(&self, name: &str) -> Option<u32> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "uint" {
            warn!(
                "Member {} is not of type uint, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 4 > raw_data.len() {
            warn!("Raw data is too small for uint at member {}", name);
            return None;
        }
        let bytes: [u8; 4] = raw_data[offset..offset + 4].try_into().ok()?;
        Some(u32::from_ne_bytes(bytes))
    }

    /// Getter for an `int` (i32, 4 bytes).
    pub fn get_int(&self, name: &str) -> Option<i32> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "int" {
            warn!(
                "Member {} is not of type int, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 4 > raw_data.len() {
            warn!("Raw data is too small for int at member {}", name);
            return None;
        }
        let bytes: [u8; 4] = raw_data[offset..offset + 4].try_into().ok()?;
        Some(i32::from_ne_bytes(bytes))
    }

    /// Getter for a `vec2` (2 f32 values, 8 bytes total).
    pub fn get_vec2(&self, name: &str) -> Option<[f32; 2]> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "vec2" {
            warn!(
                "Member {} is not of type vec2, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 8 > raw_data.len() {
            warn!("Raw data is too small for vec2 at member {}", name);
            return None;
        }
        let mut result = [0f32; 2];
        for i in 0..2 {
            let start = offset + i * 4;
            let end = start + 4;
            result[i] = f32::from_ne_bytes(raw_data[start..end].try_into().ok()?);
        }
        Some(result)
    }

    /// Getter for a `vec3` (3 f32 values, 12 bytes total).
    pub fn get_vec3(&self, name: &str) -> Option<[f32; 3]> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "vec3" {
            warn!(
                "Member {} is not of type vec3, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 12 > raw_data.len() {
            warn!("Raw data is too small for vec3 at member {}", name);
            return None;
        }
        let mut result = [0f32; 3];
        for i in 0..3 {
            let start = offset + i * 4;
            let end = start + 4;
            result[i] = f32::from_ne_bytes(raw_data[start..end].try_into().ok()?);
        }
        Some(result)
    }

    /// Getter for a `vec4` (4 f32 values, 16 bytes total).
    pub fn get_vec4(&self, name: &str) -> Option<[f32; 4]> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "vec4" {
            warn!(
                "Member {} is not of type vec4, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 16 > raw_data.len() {
            warn!("Raw data is too small for vec4 at member {}", name);
            return None;
        }
        let mut result = [0f32; 4];
        for i in 0..4 {
            let start = offset + i * 4;
            let end = start + 4;
            result[i] = f32::from_ne_bytes(raw_data[start..end].try_into().ok()?);
        }
        Some(result)
    }

    /// Getter for an `ivec2` (2 i32 values, 8 bytes total).
    pub fn get_ivec2(&self, name: &str) -> Option<[i32; 2]> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "ivec2" {
            warn!(
                "Member {} is not of type ivec2, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 8 > raw_data.len() {
            warn!("Raw data is too small for ivec2 at member {}", name);
            return None;
        }
        let mut result = [0i32; 2];
        for i in 0..2 {
            let start = offset + i * 4;
            let end = start + 4;
            result[i] = i32::from_ne_bytes(raw_data[start..end].try_into().ok()?);
        }
        Some(result)
    }

    /// Getter for an `ivec3` (3 i32 values, 12 bytes total).
    pub fn get_ivec3(&self, name: &str) -> Option<[i32; 3]> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "ivec3" {
            warn!(
                "Member {} is not of type ivec3, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 12 > raw_data.len() {
            warn!("Raw data is too small for ivec3 at member {}", name);
            return None;
        }
        let mut result = [0i32; 3];
        for i in 0..3 {
            let start = offset + i * 4;
            let end = start + 4;
            result[i] = i32::from_ne_bytes(raw_data[start..end].try_into().ok()?);
        }
        Some(result)
    }

    /// Getter for an `ivec4` (4 i32 values, 16 bytes total).
    pub fn get_ivec4(&self, name: &str) -> Option<[i32; 4]> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "ivec4" {
            warn!(
                "Member {} is not of type ivec4, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 16 > raw_data.len() {
            warn!("Raw data is too small for ivec4 at member {}", name);
            return None;
        }
        let mut result = [0i32; 4];
        for i in 0..4 {
            let start = offset + i * 4;
            let end = start + 4;
            result[i] = i32::from_ne_bytes(raw_data[start..end].try_into().ok()?);
        }
        Some(result)
    }

    /// Getter for a `uvec2` (2 u32 values, 8 bytes total).
    pub fn get_uvec2(&self, name: &str) -> Option<[u32; 2]> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "uvec2" {
            warn!(
                "Member {} is not of type uvec2, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 8 > raw_data.len() {
            warn!("Raw data is too small for uvec2 at member {}", name);
            return None;
        }
        let mut result = [0u32; 2];
        for i in 0..2 {
            let start = offset + i * 4;
            let end = start + 4;
            result[i] = u32::from_ne_bytes(raw_data[start..end].try_into().ok()?);
        }
        Some(result)
    }

    /// Getter for a `uvec3` (3 u32 values, 12 bytes total).
    pub fn get_uvec3(&self, name: &str) -> Option<[u32; 3]> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "uvec3" {
            warn!(
                "Member {} is not of type uvec3, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 12 > raw_data.len() {
            warn!("Raw data is too small for uvec3 at member {}", name);
            return None;
        }
        let mut result = [0u32; 3];
        for i in 0..3 {
            let start = offset + i * 4;
            let end = start + 4;
            result[i] = u32::from_ne_bytes(raw_data[start..end].try_into().ok()?);
        }
        Some(result)
    }

    /// Getter for a `uvec4` (4 u32 values, 16 bytes total).
    pub fn get_uvec4(&self, name: &str) -> Option<[u32; 4]> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "uvec4" {
            warn!(
                "Member {} is not of type uvec4, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 16 > raw_data.len() {
            warn!("Raw data is too small for uvec4 at member {}", name);
            return None;
        }
        let mut result = [0u32; 4];
        for i in 0..4 {
            let start = offset + i * 4;
            let end = start + 4;
            result[i] = u32::from_ne_bytes(raw_data[start..end].try_into().ok()?);
        }
        Some(result)
    }

    /// Getter for a `mat2` (2x2 matrix: 4 f32 values, 16 bytes total).
    pub fn get_mat2(&self, name: &str) -> Option<[[f32; 2]; 2]> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "mat2" {
            warn!(
                "Member {} is not of type mat2, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 16 > raw_data.len() {
            warn!("Raw data is too small for mat2 at member {}", name);
            return None;
        }
        let mut mat = [[0f32; 2]; 2];
        for i in 0..4 {
            let start = offset + i * 4;
            let end = start + 4;
            mat[i / 2][i % 2] = f32::from_ne_bytes(raw_data[start..end].try_into().ok()?);
        }
        Some(mat)
    }

    /// Getter for a `mat3` (3x3 matrix: 9 f32 values, 36 bytes total).
    pub fn get_mat3(&self, name: &str) -> Option<[[f32; 3]; 3]> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "mat3" {
            warn!(
                "Member {} is not of type mat3, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 36 > raw_data.len() {
            warn!("Raw data is too small for mat3 at member {}", name);
            return None;
        }
        let mut mat = [[0f32; 3]; 3];
        for i in 0..9 {
            let start = offset + i * 4;
            let end = start + 4;
            mat[i / 3][i % 3] = f32::from_ne_bytes(raw_data[start..end].try_into().ok()?);
        }
        Some(mat)
    }

    /// Getter for a `mat4` (4x4 matrix: 16 f32 values, 64 bytes total).
    pub fn get_mat4(&self, name: &str) -> Option<[[f32; 4]; 4]> {
        let raw_data = self.raw_data.as_ref()?;
        let member = self.layout.get_member(name)?;
        if member.type_name != "mat4" {
            warn!(
                "Member {} is not of type mat4, but {}",
                name, member.type_name
            );
            return None;
        }
        let offset = member.offset as usize;
        if offset + 64 > raw_data.len() {
            warn!("Raw data is too small for mat4 at member {}", name);
            return None;
        }
        let mut mat = [[0f32; 4]; 4];
        for i in 0..16 {
            let start = offset + i * 4;
            let end = start + 4;
            mat[i / 4][i % 4] = f32::from_ne_bytes(raw_data[start..end].try_into().ok()?);
        }
        Some(mat)
    }
}

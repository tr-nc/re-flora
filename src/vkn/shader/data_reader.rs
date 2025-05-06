#![allow(dead_code)]

use std::collections::HashMap;
use std::convert::TryInto;

use crate::vkn::{MemberLayout, PlainMemberLayout, PlainMemberType, StructMemberLayout};

use super::PlainMemberTypeWithData;

/// A tiny helper that knows how to read raw bytes into the right PlainMemberTypeWithData.
pub struct PlainMemberDataReader<'a> {
    layout: &'a PlainMemberLayout,
    bytes: &'a [u8],
}

impl<'a> PlainMemberDataReader<'a> {
    /// Create a reader for exactly the bytes corresponding to this plain member.
    pub fn new(layout: &'a PlainMemberLayout, buffer: &'a [u8]) -> Result<Self, String> {
        let offset = layout.offset as usize;
        let size = layout.size as usize; // minimal data, ignoring padding
        if buffer.len() < offset + size {
            return Err(format!(
                "Buffer too small: need {}+{} bytes, have {}",
                offset,
                size,
                buffer.len()
            ));
        }
        let bytes = &buffer[offset..offset + size];
        Ok(PlainMemberDataReader { layout, bytes })
    }

    /// Consume and interpret the bytes as the correct variant.
    pub fn read(&self) -> PlainMemberTypeWithData {
        use PlainMemberType::*;
        use PlainMemberTypeWithData as D;

        match self.layout.ty {
            Int => {
                let v = i32::from_ne_bytes(self.bytes.try_into().unwrap());
                D::Int(v)
            }
            UInt => {
                let v = u32::from_ne_bytes(self.bytes.try_into().unwrap());
                D::UInt(v)
            }
            Float => {
                let v = f32::from_ne_bytes(self.bytes.try_into().unwrap());
                D::Float(v)
            }
            Vec2 => {
                let mut a = [0.0f32; 2];
                for i in 0..2 {
                    let start = i * 4;
                    a[i] = f32::from_ne_bytes(self.bytes[start..start + 4].try_into().unwrap());
                }
                D::Vec2(a)
            }
            Vec3 => {
                let mut a = [0.0f32; 3];
                for i in 0..3 {
                    let start = i * 4;
                    a[i] = f32::from_ne_bytes(self.bytes[start..start + 4].try_into().unwrap());
                }
                D::Vec3(a)
            }
            Vec4 => {
                let mut a = [0.0f32; 4];
                for i in 0..4 {
                    let start = i * 4;
                    a[i] = f32::from_ne_bytes(self.bytes[start..start + 4].try_into().unwrap());
                }
                D::Vec4(a)
            }
            IVec2 => {
                let mut a = [0i32; 2];
                for i in 0..2 {
                    let start = i * 4;
                    a[i] = i32::from_ne_bytes(self.bytes[start..start + 4].try_into().unwrap());
                }
                D::IVec2(a)
            }
            IVec3 => {
                let mut a = [0i32; 3];
                for i in 0..3 {
                    let start = i * 4;
                    a[i] = i32::from_ne_bytes(self.bytes[start..start + 4].try_into().unwrap());
                }
                D::IVec3(a)
            }
            IVec4 => {
                let mut a = [0i32; 4];
                for i in 0..4 {
                    let start = i * 4;
                    a[i] = i32::from_ne_bytes(self.bytes[start..start + 4].try_into().unwrap());
                }
                D::IVec4(a)
            }
            UVec2 => {
                let mut a = [0u32; 2];
                for i in 0..2 {
                    let start = i * 4;
                    a[i] = u32::from_ne_bytes(self.bytes[start..start + 4].try_into().unwrap());
                }
                D::UVec2(a)
            }
            UVec3 => {
                let mut a = [0u32; 3];
                for i in 0..3 {
                    let start = i * 4;
                    a[i] = u32::from_ne_bytes(self.bytes[start..start + 4].try_into().unwrap());
                }
                D::UVec3(a)
            }
            UVec4 => {
                let mut a = [0u32; 4];
                for i in 0..4 {
                    let start = i * 4;
                    a[i] = u32::from_ne_bytes(self.bytes[start..start + 4].try_into().unwrap());
                }
                D::UVec4(a)
            }
            Mat2 => {
                // a 2×2 matrix is 4 floats in row‑major
                let mut m = [[0.0f32; 2]; 2];
                for r in 0..2 {
                    for c in 0..2 {
                        let idx = (r * 2 + c) * 4;
                        m[r][c] = f32::from_ne_bytes(self.bytes[idx..idx + 4].try_into().unwrap());
                    }
                }
                D::Mat2(m)
            }
            Mat3 => {
                let mut m = [[0.0f32; 3]; 3];
                for r in 0..3 {
                    for c in 0..3 {
                        let idx = (r * 3 + c) * 4;
                        m[r][c] = f32::from_ne_bytes(self.bytes[idx..idx + 4].try_into().unwrap());
                    }
                }
                D::Mat3(m)
            }
            Mat4 => {
                let mut m = [[0.0f32; 4]; 4];
                for r in 0..4 {
                    for c in 0..4 {
                        let idx = (r * 4 + c) * 4;
                        m[r][c] = f32::from_ne_bytes(self.bytes[idx..idx + 4].try_into().unwrap());
                    }
                }
                D::Mat4(m)
            }
            Mat3x4 => {
                let mut m = [[0.0f32; 4]; 3];
                for r in 0..3 {
                    for c in 0..4 {
                        let idx = (r * 4 + c) * 4;
                        m[r][c] = f32::from_ne_bytes(self.bytes[idx..idx + 4].try_into().unwrap());
                    }
                }
                D::Mat3x4(m)
            }
        }
    }
}

/// Reads a whole (possibly nested) struct from its raw bytes.
pub struct StructMemberDataReader<'a> {
    layout: &'a StructMemberLayout,
    buffer: &'a [u8],
}

impl<'a> StructMemberDataReader<'a> {
    /// Or build from any StructMemberLayout + raw bytes slice:
    pub fn new(layout: &'a StructMemberLayout, buffer: &'a [u8]) -> Self {
        StructMemberDataReader { layout, buffer }
    }

    /// Extract a single plain member by a dotted path:
    pub fn get_field(&self, path: &str) -> Result<PlainMemberTypeWithData, String> {
        let parts: Vec<&str> = path.split('.').collect();
        let plain_layout = self.find_plain_layout(&parts)?;
        let reader = PlainMemberDataReader::new(plain_layout, self.buffer)?;
        Ok(reader.read())
    }

    /// Recursively descend the layout to find the final PlainMemberLayout.
    fn find_plain_layout(&self, parts: &[&str]) -> Result<&'a PlainMemberLayout, String> {
        match parts {
            [leaf] => match self.layout.name_member_table.get(*leaf) {
                Some(MemberLayout::Plain(p)) => Ok(p),
                Some(MemberLayout::Struct(_)) => {
                    Err(format!("`{}` is a struct, not a plain field", leaf))
                }
                None => Err(format!(
                    "Field `{}` not found in `{}`",
                    leaf, self.layout.name
                )),
            },
            [first, rest @ ..] => {
                match self.layout.name_member_table.get(*first) {
                    Some(MemberLayout::Struct(sublayout)) => {
                        // recurse with the same buffer but a nested layout
                        StructMemberDataReader {
                            layout: sublayout,
                            buffer: self.buffer,
                        }
                        .find_plain_layout(rest)
                    }
                    Some(MemberLayout::Plain(_)) => {
                        Err(format!("`{}` is a plain member, not a struct", first))
                    }
                    None => Err(format!(
                        "Field `{}` not found in `{}`",
                        first, self.layout.name
                    )),
                }
            }
            [] => unreachable!(),
        }
    }

    /// (Optional) get _all_ leaf fields in this (sub‑)struct flat into a map.
    pub fn get_all_fields(&self) -> HashMap<String, PlainMemberTypeWithData> {
        let mut map = HashMap::new();
        self.collect_fields("", self.layout, &mut map);
        map
    }

    fn collect_fields(
        &self,
        prefix: &str,
        layout: &'a StructMemberLayout,
        out: &mut HashMap<String, PlainMemberTypeWithData>,
    ) {
        for (name, member) in &layout.name_member_table {
            let key = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}.{}", prefix, name)
            };
            match member {
                MemberLayout::Plain(p) => {
                    if let Ok(reader) = PlainMemberDataReader::new(p, self.buffer) {
                        out.insert(key, reader.read());
                    }
                }
                MemberLayout::Struct(s) => {
                    self.collect_fields(&key, s, out);
                }
            }
        }
    }
}

use std::collections::HashMap;

use crate::vkn::{Buffer, MemberLayout};

use super::{PlainMemberLayout, PlainMemberTypeWithData, StructMemberLayout};

struct PlainMemberDataBuilder<'a> {
    layout: &'a PlainMemberLayout,
    data: Option<PlainMemberTypeWithData>,
}

impl<'a> PlainMemberDataBuilder<'a> {
    pub fn from_layout(layout: &'a PlainMemberLayout) -> Self {
        Self { layout, data: None }
    }

    pub fn set_val(&mut self, plain_type_with_data: PlainMemberTypeWithData) -> Result<(), String> {
        if !plain_type_with_data.has_type(&self.layout.ty) {
            return Err(format!(
                "Member {} is not a `{:?}`, it's a `{:?}`",
                self.layout.name, self.layout.ty, plain_type_with_data
            ));
        }

        self.data = Some(plain_type_with_data);
        return Ok(());
    }

    pub fn get_data_u8(&self) -> Option<Vec<u8>> {
        let padded_size = self.layout.padded_size as usize;
        self.data.as_ref().map(|value| {
            // 1) serialize into a minimal Vec<u8>
            let mut bytes = match value {
                PlainMemberTypeWithData::Int(v) => v.to_ne_bytes().to_vec(),
                PlainMemberTypeWithData::UInt(v) => v.to_ne_bytes().to_vec(),
                PlainMemberTypeWithData::Int64(v) => v.to_ne_bytes().to_vec(),
                PlainMemberTypeWithData::UInt64(v) => v.to_ne_bytes().to_vec(),
                PlainMemberTypeWithData::Float(v) => v.to_ne_bytes().to_vec(),

                PlainMemberTypeWithData::Vec2(v) => {
                    let mut b = Vec::with_capacity(2 * 4);
                    for &x in v.iter() {
                        b.extend_from_slice(&x.to_ne_bytes());
                    }
                    b
                }
                PlainMemberTypeWithData::IVec2(v) => {
                    let mut b = Vec::with_capacity(2 * 4);
                    for &x in v.iter() {
                        b.extend_from_slice(&x.to_ne_bytes());
                    }
                    b
                }
                PlainMemberTypeWithData::UVec2(v) => {
                    let mut b = Vec::with_capacity(2 * 4);
                    for &x in v.iter() {
                        b.extend_from_slice(&x.to_ne_bytes());
                    }
                    b
                }

                PlainMemberTypeWithData::Vec3(v) => {
                    let mut b = Vec::with_capacity(3 * 4);
                    for &x in v.iter() {
                        b.extend_from_slice(&x.to_ne_bytes());
                    }
                    b
                }
                PlainMemberTypeWithData::IVec3(v) => {
                    let mut b = Vec::with_capacity(3 * 4);
                    for &x in v.iter() {
                        b.extend_from_slice(&x.to_ne_bytes());
                    }
                    b
                }
                PlainMemberTypeWithData::UVec3(v) => {
                    let mut b = Vec::with_capacity(3 * 4);
                    for &x in v.iter() {
                        b.extend_from_slice(&x.to_ne_bytes());
                    }
                    b
                }

                PlainMemberTypeWithData::Vec4(v) => {
                    let mut b = Vec::with_capacity(4 * 4);
                    for &x in v.iter() {
                        b.extend_from_slice(&x.to_ne_bytes());
                    }
                    b
                }
                PlainMemberTypeWithData::IVec4(v) => {
                    let mut b = Vec::with_capacity(4 * 4);
                    for &x in v.iter() {
                        b.extend_from_slice(&x.to_ne_bytes());
                    }
                    b
                }
                PlainMemberTypeWithData::UVec4(v) => {
                    let mut b = Vec::with_capacity(4 * 4);
                    for &x in v.iter() {
                        b.extend_from_slice(&x.to_ne_bytes());
                    }
                    b
                }

                PlainMemberTypeWithData::Mat2(m) => {
                    let mut b = Vec::with_capacity(2 * 2 * 4);
                    for row in m.iter().flat_map(|r| r.iter()) {
                        b.extend_from_slice(&row.to_ne_bytes());
                    }
                    b
                }
                PlainMemberTypeWithData::Mat3(m) => {
                    let mut b = Vec::with_capacity(3 * 3 * 4);
                    for row in m.iter().flat_map(|r| r.iter()) {
                        b.extend_from_slice(&row.to_ne_bytes());
                    }
                    b
                }
                PlainMemberTypeWithData::Mat4(m) => {
                    let mut b = Vec::with_capacity(4 * 4 * 4);
                    for row in m.iter().flat_map(|r| r.iter()) {
                        b.extend_from_slice(&row.to_ne_bytes());
                    }
                    b
                }
                PlainMemberTypeWithData::Mat3x4(m) => {
                    let mut b = Vec::with_capacity(3 * 4 * 4);
                    for row in m.iter().flat_map(|r| r.iter()) {
                        b.extend_from_slice(&row.to_ne_bytes());
                    }
                    b
                }
            };

            // 2) pad with zeros or truncate to exactly padded_size
            if bytes.len() < padded_size {
                bytes.resize(padded_size, 0);
            } else if bytes.len() > padded_size {
                bytes.truncate(padded_size);
            }

            bytes
        })
    }
}

pub struct StructMemberDataBuilder<'a> {
    layout: &'a StructMemberLayout,
    plain_member_builders: HashMap<String, PlainMemberDataBuilder<'a>>,
    struct_member_builders: HashMap<String, StructMemberDataBuilder<'a>>,
}

impl<'a> StructMemberDataBuilder<'a> {
    pub fn from_buffer(buffer: &'a Buffer) -> Self {
        let layout = &buffer
            .get_layout()
            .expect("The buffer doesn't have a layout")
            .root_member;
        Self::from_layout(layout)
    }

    pub fn from_layout(layout: &'a StructMemberLayout) -> Self {
        let mut plain_member_builders = HashMap::new();
        let mut struct_member_builders = HashMap::new();

        for (_, member) in layout.name_member_table.iter() {
            match member {
                MemberLayout::Plain(plain_layout) => {
                    let pdb = PlainMemberDataBuilder::from_layout(plain_layout);
                    plain_member_builders.insert(plain_layout.name.clone(), pdb);
                }
                MemberLayout::Struct(struct_layout) => {
                    let sdb = StructMemberDataBuilder::from_layout(struct_layout);
                    struct_member_builders.insert(struct_layout.name.clone(), sdb);
                }
            }
        }

        StructMemberDataBuilder {
            layout,
            plain_member_builders,
            struct_member_builders,
        }
    }

    /// Set a plain‐typed field by a path like `"foo.bar.baz"`.
    /// Will descend into nested struct builders as needed.
    pub fn set_field(
        &mut self,
        field_path: &str,
        value: PlainMemberTypeWithData,
    ) -> Result<&mut Self, String> {
        // split on dots into vector of &str
        let parts: Vec<&str> = field_path.split('.').collect();
        self.set_field_recursive(&parts, value)?;
        Ok(self)
    }

    /// internal recursive helper
    fn set_field_recursive(
        &mut self,
        parts: &[&str],
        value: PlainMemberTypeWithData,
    ) -> Result<(), String> {
        match parts {
            // leaf: try to set a plain member here
            [field_name] => {
                if let Some(plain) = self.plain_member_builders.get_mut(*field_name) {
                    plain.set_val(value)?;
                    Ok(())
                } else {
                    Err(format!(
                        "Field `{}` not found in struct `{}`, all fields: {:?}",
                        field_name,
                        self.layout.name,
                        self.plain_member_builders.keys()
                    ))
                }
            }
            // more parts: descend into a nested struct builder
            [first, rest @ ..] => {
                if let Some(nested) = self.struct_member_builders.get_mut(*first) {
                    nested.set_field_recursive(rest, value)
                } else {
                    Err(format!(
                        "Struct field `{}` not found in struct `{}`",
                        first, self.layout.name
                    ))
                }
            }
            [] => unreachable!("`parts` should never be empty"),
        }
    }

    /// Produce one flat Vec<u8> for the entire (sub‑)struct,
    /// recursively writing every plain member at its offset.
    pub fn get_data_u8(&self) -> Vec<u8> {
        let mut data = vec![0u8; self.layout.get_size_bytes() as usize];
        self.write_all_fields(&mut data);
        data
    }

    /// internal helper: write this struct’s plains, then recurse into sub‑structs
    fn write_all_fields(&self, data: &mut [u8]) {
        // 1) write immediate plain fields
        for plain_builder in self.plain_member_builders.values() {
            if let Some(bytes) = plain_builder.get_data_u8() {
                let offset = plain_builder.layout.offset as usize;
                let end = offset + bytes.len();
                data[offset..end].copy_from_slice(&bytes);
            } else {
                log::warn!(
                    "Plain field `{}` was never set, leaving zeros",
                    plain_builder.layout.name
                );
            }
        }

        // 2) recurse into each nested struct
        for nested in self.struct_member_builders.values() {
            nested.write_all_fields(data);
        }
    }
}

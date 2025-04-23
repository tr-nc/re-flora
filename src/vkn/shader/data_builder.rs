use std::collections::HashMap;

use crate::vkn::{Buffer, MemberLayout};

use super::{PlainMemberLayout, PlainMemberTypeWithData, StructMemberLayout};

pub struct PlainMemberDataBuilder<'a> {
    pub layout: &'a PlainMemberLayout,
    pub data: Option<PlainMemberTypeWithData>,
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

    pub fn get_val(&self) -> Option<PlainMemberTypeWithData> {
        self.data.clone()
    }

    pub fn get_data_u8(&self) -> Option<Vec<u8>> {
        let padded_size = self.layout.padded_size as usize;
        self.data.as_ref().map(|value| {
            // 1) serialize into a minimal Vec<u8>
            let mut bytes = match value {
                PlainMemberTypeWithData::Int(v) => v.to_ne_bytes().to_vec(),
                PlainMemberTypeWithData::UInt(v) => v.to_ne_bytes().to_vec(),
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
    pub layout: &'a StructMemberLayout,
    pub plain_member_builders: HashMap<String, PlainMemberDataBuilder<'a>>,
    pub struct_member_builders: HashMap<String, StructMemberDataBuilder<'a>>,
}

impl<'a> StructMemberDataBuilder<'a> {
    pub fn from_struct_buffer(buffer: &'a Buffer) -> Self {
        let layout = &buffer.get_layout().unwrap().root_member;
        return Self::from_layout(layout);
    }

    pub fn from_layout(layout: &'a StructMemberLayout) -> Self {
        let mut plain_member_builders = HashMap::new();
        let mut struct_member_builders = HashMap::new();

        for (_, member) in layout.name_member_table.iter() {
            match member {
                MemberLayout::Plain(plain_member) => {
                    let pdb = PlainMemberDataBuilder::from_layout(&plain_member);
                    plain_member_builders.insert(plain_member.name.clone(), pdb);
                }
                MemberLayout::Struct(struct_member) => {
                    let sdb = StructMemberDataBuilder::from_layout(&struct_member);
                    struct_member_builders.insert(struct_member.name.clone(), sdb);
                }
            };
        }

        Self {
            layout,
            plain_member_builders,
            struct_member_builders,
        }
    }

    pub fn set_field(
        &mut self,
        field_name: &str,
        value: PlainMemberTypeWithData,
    ) -> Result<&mut Self, String> {
        if let Some(field) = self.plain_member_builders.get_mut(field_name) {
            field.set_val(value)?;
            return Ok(self);
        } else {
            return Err(format!(
                "Field {} not found in struct {}",
                field_name, self.layout.name
            ));
        }
    }

    pub fn get_data_u8(&self) -> Vec<u8> {
        let mut data: Vec<u8> = vec![0; self.layout.get_size() as usize];

        for (_, data_builder) in self.plain_member_builders.iter() {
            let bytes = data_builder.get_data_u8();
            if bytes.is_none() {
                log::error!("Member {} is None", data_builder.layout.name);
                // continue leaves the data as 0
                continue;
            }
            let bytes = bytes.unwrap();

            // let padded_size = data_builder.layout.padded_size as usize;
            let offset = data_builder.layout.offset as usize;

            // Copy the bytes into the correct position in the data vector
            for i in 0..bytes.len() {
                data[offset + i] = bytes[i];
            }
        }
        return data;
    }
}

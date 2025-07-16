[02:17:52.273 DEBUG re_flora::tracer] player_collider_sm: ShaderModule {
    module_name: "player_collider.comp",
    bindings_count: 5,
    bindings: [
        ReflectDescriptorBinding {
            spirv_id: 1043,
            name: "player_collider_info",
            binding: 0,
            input_attachment_index: 0,
            set: 0,
            descriptor_type: UniformBuffer,
            resource_type: ConstantBufferView,
            image: ReflectImageTraits {
                dim: Type1d,
                depth: 0,
                arrayed: 0,
                ms: 0,
                sampled: 0,
                image_format: Undefined,
            },
            block: ReflectBlockVariable {
                spirv_id: 0,
                name: "player_collider_info",
                offset: 0,
                absolute_offset: 0,
                size: 32,
                padded_size: 32,
                decoration_flags: NONE,
                numeric: ReflectNumericTraits {
                    scalar: ReflectNumericTraitsScalar {
                        width: 0,
                        signedness: 0,
                    },
                    vector: ReflectNumericTraitsVector {
                        component_count: 0,
                    },
                    matrix: ReflectNumericTraitsMatrix {
                        column_count: 0,
                        row_count: 0,
                        stride: 0,
                    },
                },
                array: ReflectArrayTraits {
                    dims: [],
                    stride: 0,
                },
                members: [
                    ReflectBlockVariable {
                        spirv_id: 0,
                        name: "player_pos",
                        offset: 0,
                        absolute_offset: 0,
                        size: 12,
                        padded_size: 16,
                        decoration_flags: NONE,
                        numeric: ReflectNumericTraits {
                            scalar: ReflectNumericTraitsScalar {
                                width: 32,
                                signedness: 0,
                            },
                            vector: ReflectNumericTraitsVector {
                                component_count: 3,
                            },
                            matrix: ReflectNumericTraitsMatrix {
                                column_count: 0,
                                row_count: 0,
                                stride: 0,
                            },
                        },
                        array: ReflectArrayTraits {
                            dims: [],
                            stride: 0,
                        },
                        members: [],
                        type_description: Some(
                            ReflectTypeDescription {
                                id: 7,
                                op: ReflectOp(
                                    TypeVector,
                                ),
                                type_name: "",
                                struct_member_name: "player_pos",
                                storage_class: UniformConstant,
                                type_flags: FLOAT | VECTOR,
                                decoration_flags: NONE,
                                traits: ReflectTypeDescriptionTraits {
                                    numeric: ReflectNumericTraits {
                                        scalar: ReflectNumericTraitsScalar {
                                            width: 32,
                                            signedness: 0,
                                        },
                                        vector: ReflectNumericTraitsVector {
                                            component_count: 3,
                                        },
                                        matrix: ReflectNumericTraitsMatrix {
                                            column_count: 0,
                                            row_count: 0,
                                            stride: 0,
                                        },
                                    },
                                    image: ReflectImageTraits {
                                        dim: Type1d,
                                        depth: 0,
                                        arrayed: 0,
                                        ms: 0,
                                        sampled: 0,
                                        image_format: Undefined,
                                    },
                                    array: ReflectArrayTraits {
                                        dims: [],
                                        stride: 0,
                                    },
                                },
                                members: [],
                            },
                        ),
                    },
                    ReflectBlockVariable {
                        spirv_id: 0,
                        name: "camera_front",
                        offset: 16,
                        absolute_offset: 16,
                        size: 12,
                        padded_size: 16,
                        decoration_flags: NONE,
                        numeric: ReflectNumericTraits {
                            scalar: ReflectNumericTraitsScalar {
                                width: 32,
                                signedness: 0,
                            },
                            vector: ReflectNumericTraitsVector {
                                component_count: 3,
                            },
                            matrix: ReflectNumericTraitsMatrix {
                                column_count: 0,
                                row_count: 0,
                                stride: 0,
                            },
                        },
                        array: ReflectArrayTraits {
                            dims: [],
                            stride: 0,
                        },
                        members: [],
                        type_description: Some(
                            ReflectTypeDescription {
                                id: 7,
                                op: ReflectOp(
                                    TypeVector,
                                ),
                                type_name: "",
                                struct_member_name: "camera_front",
                                storage_class: UniformConstant,
                                type_flags: FLOAT | VECTOR,
                                decoration_flags: NONE,
                                traits: ReflectTypeDescriptionTraits {
                                    numeric: ReflectNumericTraits {
                                        scalar: ReflectNumericTraitsScalar {
                                            width: 32,
                                            signedness: 0,
                                        },
                                        vector: ReflectNumericTraitsVector {
                                            component_count: 3,
                                        },
                                        matrix: ReflectNumericTraitsMatrix {
                                            column_count: 0,
                                            row_count: 0,
                                            stride: 0,
                                        },
                                    },
                                    image: ReflectImageTraits {
                                        dim: Type1d,
                                        depth: 0,
                                        arrayed: 0,
                                        ms: 0,
                                        sampled: 0,
                                        image_format: Undefined,
                                    },
                                    array: ReflectArrayTraits {
                                        dims: [],
                                        stride: 0,
                                    },
                                },
                                members: [],
                            },
                        ),
                    },
                ],
                type_description: Some(
                    ReflectTypeDescription {
                        id: 1041,
                        op: ReflectOp(
                            TypeStruct,
                        ),
                        type_name: "U_PlayerColliderInfo",
                        struct_member_name: "",
                        storage_class: Undefined,
                        type_flags: EXTERNAL_BLOCK | STRUCT,
                        decoration_flags: BLOCK,
                        traits: ReflectTypeDescriptionTraits {
                            numeric: ReflectNumericTraits {
                                scalar: ReflectNumericTraitsScalar {
                                    width: 0,
                                    signedness: 0,
                                },
                                vector: ReflectNumericTraitsVector {
                                    component_count: 0,
                                },
                                matrix: ReflectNumericTraitsMatrix {
                                    column_count: 0,
                                    row_count: 0,
                                    stride: 0,
                                },
                            },
                            image: ReflectImageTraits {
                                dim: Type1d,
                                depth: 0,
                                arrayed: 0,
                                ms: 0,
                                sampled: 0,
                                image_format: Undefined,
                            },
                            array: ReflectArrayTraits {
                                dims: [],
                                stride: 0,
                            },
                        },
                        members: [
                            ReflectTypeDescription {
                                id: 7,
                                op: ReflectOp(
                                    TypeVector,
                                ),
                                type_name: "",
                                struct_member_name: "player_pos",
                                storage_class: UniformConstant,
                                type_flags: FLOAT | VECTOR,
                                decoration_flags: NONE,
                                traits: ReflectTypeDescriptionTraits {
                                    numeric: ReflectNumericTraits {
                                        scalar: ReflectNumericTraitsScalar {
                                            width: 32,
                                            signedness: 0,
                                        },
                                        vector: ReflectNumericTraitsVector {
                                            component_count: 3,
                                        },
                                        matrix: ReflectNumericTraitsMatrix {
                                            column_count: 0,
                                            row_count: 0,
                                            stride: 0,
                                        },
                                    },
                                    image: ReflectImageTraits {
                                        dim: Type1d,
                                        depth: 0,
                                        arrayed: 0,
                                        ms: 0,
                                        sampled: 0,
                                        image_format: Undefined,
                                    },
                                    array: ReflectArrayTraits {
                                        dims: [],
                                        stride: 0,
                                    },
                                },
                                members: [],
                            },
                            ReflectTypeDescription {
                                id: 7,
                                op: ReflectOp(
                                    TypeVector,
                                ),
                                type_name: "",
                                struct_member_name: "camera_front",
                                storage_class: UniformConstant,
                                type_flags: FLOAT | VECTOR,
                                decoration_flags: NONE,
                                traits: ReflectTypeDescriptionTraits {
                                    numeric: ReflectNumericTraits {
                                        scalar: ReflectNumericTraitsScalar {
                                            width: 32,
                                            signedness: 0,
                                        },
                                        vector: ReflectNumericTraitsVector {
                                            component_count: 3,
                                        },
                                        matrix: ReflectNumericTraitsMatrix {
                                            column_count: 0,
                                            row_count: 0,
                                            stride: 0,
                                        },
                                    },
                                    image: ReflectImageTraits {
                                        dim: Type1d,
                                        depth: 0,
                                        arrayed: 0,
                                        ms: 0,
                                        sampled: 0,
                                        image_format: Undefined,
                                    },
                                    array: ReflectArrayTraits {
                                        dims: [],
                                        stride: 0,
                                    },
                                },
                                members: [],
                            },
                        ],
                    },
                ),
            },
            array: ReflectBindingArrayTraits {
                dims: [],
            },
            count: 1,
            uav_counter_id: 4294967295,
            uav_counter_binding: None,
            type_description: Some(
                ReflectTypeDescription {
                    id: 1041,
                    op: ReflectOp(
                        TypeStruct,
                    ),
                    type_name: "U_PlayerColliderInfo",
                    struct_member_name: "",
                    storage_class: Undefined,
                    type_flags: EXTERNAL_BLOCK | STRUCT,
                    decoration_flags: BLOCK,
                    traits: ReflectTypeDescriptionTraits {
                        numeric: ReflectNumericTraits {
                            scalar: ReflectNumericTraitsScalar {
                                width: 0,
                                signedness: 0,
                            },
                            vector: ReflectNumericTraitsVector {
                                component_count: 0,
                            },
                            matrix: ReflectNumericTraitsMatrix {
                                column_count: 0,
                                row_count: 0,
                                stride: 0,
                            },
                        },
                        image: ReflectImageTraits {
                            dim: Type1d,
                            depth: 0,
                            arrayed: 0,
                            ms: 0,
                            sampled: 0,
                            image_format: Undefined,
                        },
                        array: ReflectArrayTraits {
                            dims: [],
                            stride: 0,
                        },
                    },
                    members: [
                        ReflectTypeDescription {
                            id: 7,
                            op: ReflectOp(
                                TypeVector,
                            ),
                            type_name: "",
                            struct_member_name: "player_pos",
                            storage_class: UniformConstant,
                            type_flags: FLOAT | VECTOR,
                            decoration_flags: NONE,
                            traits: ReflectTypeDescriptionTraits {
                                numeric: ReflectNumericTraits {
                                    scalar: ReflectNumericTraitsScalar {
                                        width: 32,
                                        signedness: 0,
                                    },
                                    vector: ReflectNumericTraitsVector {
                                        component_count: 3,
                                    },
                                    matrix: ReflectNumericTraitsMatrix {
                                        column_count: 0,
                                        row_count: 0,
                                        stride: 0,
                                    },
                                },
                                image: ReflectImageTraits {
                                    dim: Type1d,
                                    depth: 0,
                                    arrayed: 0,
                                    ms: 0,
                                    sampled: 0,
                                    image_format: Undefined,
                                },
                                array: ReflectArrayTraits {
                                    dims: [],
                                    stride: 0,
                                },
                            },
                            members: [],
                        },
                        ReflectTypeDescription {
                            id: 7,
                            op: ReflectOp(
                                TypeVector,
                            ),
                            type_name: "",
                            struct_member_name: "camera_front",
                            storage_class: UniformConstant,
                            type_flags: FLOAT | VECTOR,
                            decoration_flags: NONE,
                            traits: ReflectTypeDescriptionTraits {
                                numeric: ReflectNumericTraits {
                                    scalar: ReflectNumericTraitsScalar {
                                        width: 32,
                                        signedness: 0,
                                    },
                                    vector: ReflectNumericTraitsVector {
                                        component_count: 3,
                                    },
                                    matrix: ReflectNumericTraitsMatrix {
                                        column_count: 0,
                                        row_count: 0,
                                        stride: 0,
                                    },
                                },
                                image: ReflectImageTraits {
                                    dim: Type1d,
                                    depth: 0,
                                    arrayed: 0,
                                    ms: 0,
                                    sampled: 0,
                                    image_format: Undefined,
                                },
                                array: ReflectArrayTraits {
                                    dims: [],
                                    stride: 0,
                                },
                            },
                            members: [],
                        },
                    ],
                },
            ),
            word_offset: (
                1366,
                1362,
            ),
            internal_data: 0x000001f660934a30,
        },
        ReflectDescriptorBinding {
            spirv_id: 306,
            name: "contree_node_data",
            binding: 1,
            input_attachment_index: 0,
            set: 0,
            descriptor_type: StorageBuffer,
            resource_type: ShaderResourceView,
            image: ReflectImageTraits {
                dim: Type1d,
                depth: 0,
                arrayed: 0,
                ms: 0,
                sampled: 0,
                image_format: Undefined,
            },
            block: ReflectBlockVariable {
                spirv_id: 0,
                name: "contree_node_data",
                offset: 0,
                absolute_offset: 0,
                size: 0,
                padded_size: 0,
                decoration_flags: NON_WRITABLE,
                numeric: ReflectNumericTraits {
                    scalar: ReflectNumericTraitsScalar {
                        width: 0,
                        signedness: 0,
                    },
                    vector: ReflectNumericTraitsVector {
                        component_count: 0,
                    },
                    matrix: ReflectNumericTraitsMatrix {
                        column_count: 0,
                        row_count: 0,
                        stride: 0,
                    },
                },
                array: ReflectArrayTraits {
                    dims: [],
                    stride: 0,
                },
                members: [
                    ReflectBlockVariable {
                        spirv_id: 0,
                        name: "data",
                        offset: 0,
                        absolute_offset: 0,
                        size: 16,
                        padded_size: 16,
                        decoration_flags: NON_WRITABLE,
                        numeric: ReflectNumericTraits {
                            scalar: ReflectNumericTraitsScalar {
                                width: 0,
                                signedness: 0,
                            },
                            vector: ReflectNumericTraitsVector {
                                component_count: 0,
                            },
                            matrix: ReflectNumericTraitsMatrix {
                                column_count: 0,
                                row_count: 0,
                                stride: 0,
                            },
                        },
                        array: ReflectArrayTraits {
                            dims: [],
                            stride: 0,
                        },
                        members: [
                            ReflectBlockVariable {
                                spirv_id: 0,
                                name: "packed_0",
                                offset: 0,
                                absolute_offset: 0,
                                size: 4,
                                padded_size: 4,
                                decoration_flags: NONE,
                                numeric: ReflectNumericTraits {
                                    scalar: ReflectNumericTraitsScalar {
                                        width: 32,
                                        signedness: 0,
                                    },
                                    vector: ReflectNumericTraitsVector {
                                        component_count: 0,
                                    },
                                    matrix: ReflectNumericTraitsMatrix {
                                        column_count: 0,
                                        row_count: 0,
                                        stride: 0,
                                    },
                                },
                                array: ReflectArrayTraits {
                                    dims: [],
                                    stride: 0,
                                },
                                members: [],
                                type_description: Some(
                                    ReflectTypeDescription {
                                        id: 29,
                                        op: ReflectOp(
                                            TypeInt,
                                        ),
                                        type_name: "",
                                        struct_member_name: "packed_0",
                                        storage_class: UniformConstant,
                                        type_flags: INT,
                                        decoration_flags: NONE,
                                        traits: ReflectTypeDescriptionTraits {
                                            numeric: ReflectNumericTraits {
                                                scalar: ReflectNumericTraitsScalar {
                                                    width: 32,
                                                    signedness: 0,
                                                },
                                                vector: ReflectNumericTraitsVector {
                                                    component_count: 0,
                                                },
                                                matrix: ReflectNumericTraitsMatrix {
                                                    column_count: 0,
                                                    row_count: 0,
                                                    stride: 0,
                                                },
                                            },
                                            image: ReflectImageTraits {
                                                dim: Type1d,
                                                depth: 0,
                                                arrayed: 0,
                                                ms: 0,
                                                sampled: 0,
                                                image_format: Undefined,
                                            },
                                            array: ReflectArrayTraits {
                                                dims: [],
                                                stride: 0,
                                            },
                                        },
                                        members: [],
                                    },
                                ),
                            },
                            ReflectBlockVariable {
                                spirv_id: 0,
                                name: "child_mask",
                                offset: 8,
                                absolute_offset: 0,
                                size: 8,
                                padded_size: 8,
                                decoration_flags: NONE,
                                numeric: ReflectNumericTraits {
                                    scalar: ReflectNumericTraitsScalar {
                                        width: 64,
                                        signedness: 0,
                                    },
                                    vector: ReflectNumericTraitsVector {
                                        component_count: 0,
                                    },
                                    matrix: ReflectNumericTraitsMatrix {
                                        column_count: 0,
                                        row_count: 0,
                                        stride: 0,
                                    },
                                },
                                array: ReflectArrayTraits {
                                    dims: [],
                                    stride: 0,
                                },
                                members: [],
                                type_description: Some(
                                    ReflectTypeDescription {
                                        id: 27,
                                        op: ReflectOp(
                                            TypeInt,
                                        ),
                                        type_name: "",
                                        struct_member_name: "child_mask",
                                        storage_class: UniformConstant,
                                        type_flags: INT,
                                        decoration_flags: NONE,
                                        traits: ReflectTypeDescriptionTraits {
                                            numeric: ReflectNumericTraits {
                                                scalar: ReflectNumericTraitsScalar {
                                                    width: 64,
                                                    signedness: 0,
                                                },
                                                vector: ReflectNumericTraitsVector {
                                                    component_count: 0,
                                                },
                                                matrix: ReflectNumericTraitsMatrix {
                                                    column_count: 0,
                                                    row_count: 0,
                                                    stride: 0,
                                                },
                                            },
                                            image: ReflectImageTraits {
                                                dim: Type1d,
                                                depth: 0,
                                                arrayed: 0,
                                                ms: 0,
                                                sampled: 0,
                                                image_format: Undefined,
                                            },
                                            array: ReflectArrayTraits {
                                                dims: [],
                                                stride: 0,
                                            },
                                        },
                                        members: [],
                                    },
                                ),
                            },
                        ],
                        type_description: Some(
                            ReflectTypeDescription {
                                id: 303,
                                op: ReflectOp(
                                    TypeRuntimeArray,
                                ),
                                type_name: "ContreeNode",
                                struct_member_name: "data",
                                storage_class: UniformConstant,
                                type_flags: EXTERNAL_BLOCK | STRUCT | ARRAY,
                                decoration_flags: NONE,
                                traits: ReflectTypeDescriptionTraits {
                                    numeric: ReflectNumericTraits {
                                        scalar: ReflectNumericTraitsScalar {
                                            width: 0,
                                            signedness: 0,
                                        },
                                        vector: ReflectNumericTraitsVector {
                                            component_count: 0,
                                        },
                                        matrix: ReflectNumericTraitsMatrix {
                                            column_count: 0,
                                            row_count: 0,
                                            stride: 0,
                                        },
                                    },
                                    image: ReflectImageTraits {
                                        dim: Type1d,
                                        depth: 0,
                                        arrayed: 0,
                                        ms: 0,
                                        sampled: 0,
                                        image_format: Undefined,
                                    },
                                    array: ReflectArrayTraits {
                                        dims: [
                                            0,
                                        ],
                                        stride: 16,
                                    },
                                },
                                members: [
                                    ReflectTypeDescription {
                                        id: 29,
                                        op: ReflectOp(
                                            TypeInt,
                                        ),
                                        type_name: "",
                                        struct_member_name: "packed_0",
                                        storage_class: UniformConstant,
                                        type_flags: INT,
                                        decoration_flags: NONE,
                                        traits: ReflectTypeDescriptionTraits {
                                            numeric: ReflectNumericTraits {
                                                scalar: ReflectNumericTraitsScalar {
                                                    width: 32,
                                                    signedness: 0,
                                                },
                                                vector: ReflectNumericTraitsVector {
                                                    component_count: 0,
                                                },
                                                matrix: ReflectNumericTraitsMatrix {
                                                    column_count: 0,
                                                    row_count: 0,
                                                    stride: 0,
                                                },
                                            },
                                            image: ReflectImageTraits {
                                                dim: Type1d,
                                                depth: 0,
                                                arrayed: 0,
                                                ms: 0,
                                                sampled: 0,
                                                image_format: Undefined,
                                            },
                                            array: ReflectArrayTraits {
                                                dims: [],
                                                stride: 0,
                                            },
                                        },
                                        members: [],
                                    },
                                    ReflectTypeDescription {
                                        id: 27,
                                        op: ReflectOp(
                                            TypeInt,
                                        ),
                                        type_name: "",
                                        struct_member_name: "child_mask",
                                        storage_class: UniformConstant,
                                        type_flags: INT,
                                        decoration_flags: NONE,
                                        traits: ReflectTypeDescriptionTraits {
                                            numeric: ReflectNumericTraits {
                                                scalar: ReflectNumericTraitsScalar {
                                                    width: 64,
                                                    signedness: 0,
                                                },
                                                vector: ReflectNumericTraitsVector {
                                                    component_count: 0,
                                                },
                                                matrix: ReflectNumericTraitsMatrix {
                                                    column_count: 0,
                                                    row_count: 0,
                                                    stride: 0,
                                                },
                                            },
                                            image: ReflectImageTraits {
                                                dim: Type1d,
                                                depth: 0,
                                                arrayed: 0,
                                                ms: 0,
                                                sampled: 0,
                                                image_format: Undefined,
                                            },
                                            array: ReflectArrayTraits {
                                                dims: [],
                                                stride: 0,
                                            },
                                        },
                                        members: [],
                                    },
                                ],
                            },
                        ),
                    },
                ],
                type_description: Some(
                    ReflectTypeDescription {
                        id: 304,
                        op: ReflectOp(
                            TypeStruct,
                        ),
                        type_name: "B_ContreeNodeData",
                        struct_member_name: "",
                        storage_class: Undefined,
                        type_flags: EXTERNAL_BLOCK | STRUCT,
                        decoration_flags: BLOCK,
                        traits: ReflectTypeDescriptionTraits {
                            numeric: ReflectNumericTraits {
                                scalar: ReflectNumericTraitsScalar {
                                    width: 0,
                                    signedness: 0,
                                },
                                vector: ReflectNumericTraitsVector {
                                    component_count: 0,
                                },
                                matrix: ReflectNumericTraitsMatrix {
                                    column_count: 0,
                                    row_count: 0,
                                    stride: 0,
                                },
                            },
                            image: ReflectImageTraits {
                                dim: Type1d,
                                depth: 0,
                                arrayed: 0,
                                ms: 0,
                                sampled: 0,
                                image_format: Undefined,
                            },
                            array: ReflectArrayTraits {
                                dims: [],
                                stride: 0,
                            },
                        },
                        members: [
                            ReflectTypeDescription {
                                id: 303,
                                op: ReflectOp(
                                    TypeRuntimeArray,
                                ),
                                type_name: "ContreeNode",
                                struct_member_name: "data",
                                storage_class: UniformConstant,
                                type_flags: EXTERNAL_BLOCK | STRUCT | ARRAY,
                                decoration_flags: NONE,
                                traits: ReflectTypeDescriptionTraits {
                                    numeric: ReflectNumericTraits {
                                        scalar: ReflectNumericTraitsScalar {
                                            width: 0,
                                            signedness: 0,
                                        },
                                        vector: ReflectNumericTraitsVector {
                                            component_count: 0,
                                        },
                                        matrix: ReflectNumericTraitsMatrix {
                                            column_count: 0,
                                            row_count: 0,
                                            stride: 0,
                                        },
                                    },
                                    image: ReflectImageTraits {
                                        dim: Type1d,
                                        depth: 0,
                                        arrayed: 0,
                                        ms: 0,
                                        sampled: 0,
                                        image_format: Undefined,
                                    },
                                    array: ReflectArrayTraits {
                                        dims: [
                                            0,
                                        ],
                                        stride: 16,
                                    },
                                },
                                members: [
                                    ReflectTypeDescription {
                                        id: 29,
                                        op: ReflectOp(
                                            TypeInt,
                                        ),
                                        type_name: "",
                                        struct_member_name: "packed_0",
                                        storage_class: UniformConstant,
                                        type_flags: INT,
                                        decoration_flags: NONE,
                                        traits: ReflectTypeDescriptionTraits {
                                            numeric: ReflectNumericTraits {
                                                scalar: ReflectNumericTraitsScalar {
                                                    width: 32,
                                                    signedness: 0,
                                                },
                                                vector: ReflectNumericTraitsVector {
                                                    component_count: 0,
                                                },
                                                matrix: ReflectNumericTraitsMatrix {
                                                    column_count: 0,
                                                    row_count: 0,
                                                    stride: 0,
                                                },
                                            },
                                            image: ReflectImageTraits {
                                                dim: Type1d,
                                                depth: 0,
                                                arrayed: 0,
                                                ms: 0,
                                                sampled: 0,
                                                image_format: Undefined,
                                            },
                                            array: ReflectArrayTraits {
                                                dims: [],
                                                stride: 0,
                                            },
                                        },
                                        members: [],
                                    },
                                    ReflectTypeDescription {
                                        id: 27,
                                        op: ReflectOp(
                                            TypeInt,
                                        ),
                                        type_name: "",
                                        struct_member_name: "child_mask",
                                        storage_class: UniformConstant,
                                        type_flags: INT,
                                        decoration_flags: NONE,
                                        traits: ReflectTypeDescriptionTraits {
                                            numeric: ReflectNumericTraits {
                                                scalar: ReflectNumericTraitsScalar {
                                                    width: 64,
                                                    signedness: 0,
                                                },
                                                vector: ReflectNumericTraitsVector {
                                                    component_count: 0,
                                                },
                                                matrix: ReflectNumericTraitsMatrix {
                                                    column_count: 0,
                                                    row_count: 0,
                                                    stride: 0,
                                                },
                                            },
                                            image: ReflectImageTraits {
                                                dim: Type1d,
                                                depth: 0,
                                                arrayed: 0,
                                                ms: 0,
                                                sampled: 0,
                                                image_format: Undefined,
                                            },
                                            array: ReflectArrayTraits {
                                                dims: [],
                                                stride: 0,
                                            },
                                        },
                                        members: [],
                                    },
                                ],
                            },
                        ],
                    },
                ),
            },
            array: ReflectBindingArrayTraits {
                dims: [],
            },
            count: 1,
            uav_counter_id: 4294967295,
            uav_counter_binding: None,
            type_description: Some(
                ReflectTypeDescription {
                    id: 304,
                    op: ReflectOp(
                        TypeStruct,
                    ),
                    type_name: "B_ContreeNodeData",
                    struct_member_name: "",
                    storage_class: Undefined,
                    type_flags: EXTERNAL_BLOCK | STRUCT,
                    decoration_flags: BLOCK,
                    traits: ReflectTypeDescriptionTraits {
                        numeric: ReflectNumericTraits {
                            scalar: ReflectNumericTraitsScalar {
                                width: 0,
                                signedness: 0,
                            },
                            vector: ReflectNumericTraitsVector {
                                component_count: 0,
                            },
                            matrix: ReflectNumericTraitsMatrix {
                                column_count: 0,
                                row_count: 0,
                                stride: 0,
                            },
                        },
                        image: ReflectImageTraits {
                            dim: Type1d,
                            depth: 0,
                            arrayed: 0,
                            ms: 0,
                            sampled: 0,
                            image_format: Undefined,
                        },
                        array: ReflectArrayTraits {
                            dims: [],
                            stride: 0,
                        },
                    },
                    members: [
                        ReflectTypeDescription {
                            id: 303,
                            op: ReflectOp(
                                TypeRuntimeArray,
                            ),
                            type_name: "ContreeNode",
                            struct_member_name: "data",
                            storage_class: UniformConstant,
                            type_flags: EXTERNAL_BLOCK | STRUCT | ARRAY,
                            decoration_flags: NONE,
                            traits: ReflectTypeDescriptionTraits {
                                numeric: ReflectNumericTraits {
                                    scalar: ReflectNumericTraitsScalar {
                                        width: 0,
                                        signedness: 0,
                                    },
                                    vector: ReflectNumericTraitsVector {
                                        component_count: 0,
                                    },
                                    matrix: ReflectNumericTraitsMatrix {
                                        column_count: 0,
                                        row_count: 0,
                                        stride: 0,
                                    },
                                },
                                image: ReflectImageTraits {
                                    dim: Type1d,
                                    depth: 0,
                                    arrayed: 0,
                                    ms: 0,
                                    sampled: 0,
                                    image_format: Undefined,
                                },
                                array: ReflectArrayTraits {
                                    dims: [
                                        0,
                                    ],
                                    stride: 16,
                                },
                            },
                            members: [
                                ReflectTypeDescription {
                                    id: 29,
                                    op: ReflectOp(
                                        TypeInt,
                                    ),
                                    type_name: "",
                                    struct_member_name: "packed_0",
                                    storage_class: UniformConstant,
                                    type_flags: INT,
                                    decoration_flags: NONE,
                                    traits: ReflectTypeDescriptionTraits {
                                        numeric: ReflectNumericTraits {
                                            scalar: ReflectNumericTraitsScalar {
                                                width: 32,
                                                signedness: 0,
                                            },
                                            vector: ReflectNumericTraitsVector {
                                                component_count: 0,
                                            },
                                            matrix: ReflectNumericTraitsMatrix {
                                                column_count: 0,
                                                row_count: 0,
                                                stride: 0,
                                            },
                                        },
                                        image: ReflectImageTraits {
                                            dim: Type1d,
                                            depth: 0,
                                            arrayed: 0,
                                            ms: 0,
                                            sampled: 0,
                                            image_format: Undefined,
                                        },
                                        array: ReflectArrayTraits {
                                            dims: [],
                                            stride: 0,
                                        },
                                    },
                                    members: [],
                                },
                                ReflectTypeDescription {
                                    id: 27,
                                    op: ReflectOp(
                                        TypeInt,
                                    ),
                                    type_name: "",
                                    struct_member_name: "child_mask",
                                    storage_class: UniformConstant,
                                    type_flags: INT,
                                    decoration_flags: NONE,
                                    traits: ReflectTypeDescriptionTraits {
                                        numeric: ReflectNumericTraits {
                                            scalar: ReflectNumericTraitsScalar {
                                                width: 64,
                                                signedness: 0,
                                            },
                                            vector: ReflectNumericTraitsVector {
                                                component_count: 0,
                                            },
                                            matrix: ReflectNumericTraitsMatrix {
                                                column_count: 0,
                                                row_count: 0,
                                                stride: 0,
                                            },
                                        },
                                        image: ReflectImageTraits {
                                            dim: Type1d,
                                            depth: 0,
                                            arrayed: 0,
                                            ms: 0,
                                            sampled: 0,
                                            image_format: Undefined,
                                        },
                                        array: ReflectArrayTraits {
                                            dims: [],
                                            stride: 0,
                                        },
                                    },
                                    members: [],
                                },
                            ],
                        },
                    ],
                },
            ),
            word_offset: (
                1310,
                1306,
            ),
            internal_data: 0x000001f660934c98,
        },
        ReflectDescriptorBinding {
            spirv_id: 821,
            name: "contree_leaf_data",
            binding: 2,
            input_attachment_index: 0,
            set: 0,
            descriptor_type: StorageBuffer,
            resource_type: ShaderResourceView,
            image: ReflectImageTraits {
                dim: Type1d,
                depth: 0,
                arrayed: 0,
                ms: 0,
                sampled: 0,
                image_format: Undefined,
            },
            block: ReflectBlockVariable {
                spirv_id: 0,
                name: "contree_leaf_data",
                offset: 0,
                absolute_offset: 0,
                size: 0,
                padded_size: 0,
                decoration_flags: NON_WRITABLE,
                numeric: ReflectNumericTraits {
                    scalar: ReflectNumericTraitsScalar {
                        width: 0,
                        signedness: 0,
                    },
                    vector: ReflectNumericTraitsVector {
                        component_count: 0,
                    },
                    matrix: ReflectNumericTraitsMatrix {
                        column_count: 0,
                        row_count: 0,
                        stride: 0,
                    },
                },
                array: ReflectArrayTraits {
                    dims: [],
                    stride: 0,
                },
                members: [
                    ReflectBlockVariable {
                        spirv_id: 0,
                        name: "data",
                        offset: 0,
                        absolute_offset: 0,
                        size: 0,
                        padded_size: 0,
                        decoration_flags: NON_WRITABLE,
                        numeric: ReflectNumericTraits {
                            scalar: ReflectNumericTraitsScalar {
                                width: 32,
                                signedness: 0,
                            },
                            vector: ReflectNumericTraitsVector {
                                component_count: 0,
                            },
                            matrix: ReflectNumericTraitsMatrix {
                                column_count: 0,
                                row_count: 0,
                                stride: 0,
                            },
                        },
                        array: ReflectArrayTraits {
                            dims: [],
                            stride: 0,
                        },
                        members: [],
                        type_description: Some(
                            ReflectTypeDescription {
                                id: 818,
                                op: ReflectOp(
                                    TypeRuntimeArray,
                                ),
                                type_name: "",
                                struct_member_name: "data",
                                storage_class: UniformConstant,
                                type_flags: INT | ARRAY,
                                decoration_flags: NONE,
                                traits: ReflectTypeDescriptionTraits {
                                    numeric: ReflectNumericTraits {
                                        scalar: ReflectNumericTraitsScalar {
                                            width: 32,
                                            signedness: 0,
                                        },
                                        vector: ReflectNumericTraitsVector {
                                            component_count: 0,
                                        },
                                        matrix: ReflectNumericTraitsMatrix {
                                            column_count: 0,
                                            row_count: 0,
                                            stride: 0,
                                        },
                                    },
                                    image: ReflectImageTraits {
                                        dim: Type1d,
                                        depth: 0,
                                        arrayed: 0,
                                        ms: 0,
                                        sampled: 0,
                                        image_format: Undefined,
                                    },
                                    array: ReflectArrayTraits {
                                        dims: [
                                            0,
                                        ],
                                        stride: 4,
                                    },
                                },
                                members: [],
                            },
                        ),
                    },
                ],
                type_description: Some(
                    ReflectTypeDescription {
                        id: 819,
                        op: ReflectOp(
                            TypeStruct,
                        ),
                        type_name: "B_ContreeLeafData",
                        struct_member_name: "",
                        storage_class: Undefined,
                        type_flags: EXTERNAL_BLOCK | STRUCT,
                        decoration_flags: BLOCK,
                        traits: ReflectTypeDescriptionTraits {
                            numeric: ReflectNumericTraits {
                                scalar: ReflectNumericTraitsScalar {
                                    width: 0,
                                    signedness: 0,
                                },
                                vector: ReflectNumericTraitsVector {
                                    component_count: 0,
                                },
                                matrix: ReflectNumericTraitsMatrix {
                                    column_count: 0,
                                    row_count: 0,
                                    stride: 0,
                                },
                            },
                            image: ReflectImageTraits {
                                dim: Type1d,
                                depth: 0,
                                arrayed: 0,
                                ms: 0,
                                sampled: 0,
                                image_format: Undefined,
                            },
                            array: ReflectArrayTraits {
                                dims: [],
                                stride: 0,
                            },
                        },
                        members: [
                            ReflectTypeDescription {
                                id: 818,
                                op: ReflectOp(
                                    TypeRuntimeArray,
                                ),
                                type_name: "",
                                struct_member_name: "data",
                                storage_class: UniformConstant,
                                type_flags: INT | ARRAY,
                                decoration_flags: NONE,
                                traits: ReflectTypeDescriptionTraits {
                                    numeric: ReflectNumericTraits {
                                        scalar: ReflectNumericTraitsScalar {
                                            width: 32,
                                            signedness: 0,
                                        },
                                        vector: ReflectNumericTraitsVector {
                                            component_count: 0,
                                        },
                                        matrix: ReflectNumericTraitsMatrix {
                                            column_count: 0,
                                            row_count: 0,
                                            stride: 0,
                                        },
                                    },
                                    image: ReflectImageTraits {
                                        dim: Type1d,
                                        depth: 0,
                                        arrayed: 0,
                                        ms: 0,
                                        sampled: 0,
                                        image_format: Undefined,
                                    },
                                    array: ReflectArrayTraits {
                                        dims: [
                                            0,
                                        ],
                                        stride: 4,
                                    },
                                },
                                members: [],
                            },
                        ],
                    },
                ),
            },
            array: ReflectBindingArrayTraits {
                dims: [],
            },
            count: 1,
            uav_counter_id: 4294967295,
            uav_counter_binding: None,
            type_description: Some(
                ReflectTypeDescription {
                    id: 819,
                    op: ReflectOp(
                        TypeStruct,
                    ),
                    type_name: "B_ContreeLeafData",
                    struct_member_name: "",
                    storage_class: Undefined,
                    type_flags: EXTERNAL_BLOCK | STRUCT,
                    decoration_flags: BLOCK,
                    traits: ReflectTypeDescriptionTraits {
                        numeric: ReflectNumericTraits {
                            scalar: ReflectNumericTraitsScalar {
                                width: 0,
                                signedness: 0,
                            },
                            vector: ReflectNumericTraitsVector {
                                component_count: 0,
                            },
                            matrix: ReflectNumericTraitsMatrix {
                                column_count: 0,
                                row_count: 0,
                                stride: 0,
                            },
                        },
                        image: ReflectImageTraits {
                            dim: Type1d,
                            depth: 0,
                            arrayed: 0,
                            ms: 0,
                            sampled: 0,
                            image_format: Undefined,
                        },
                        array: ReflectArrayTraits {
                            dims: [],
                            stride: 0,
                        },
                    },
                    members: [
                        ReflectTypeDescription {
                            id: 818,
                            op: ReflectOp(
                                TypeRuntimeArray,
                            ),
                            type_name: "",
                            struct_member_name: "data",
                            storage_class: UniformConstant,
                            type_flags: INT | ARRAY,
                            decoration_flags: NONE,
                            traits: ReflectTypeDescriptionTraits {
                                numeric: ReflectNumericTraits {
                                    scalar: ReflectNumericTraitsScalar {
                                        width: 32,
                                        signedness: 0,
                                    },
                                    vector: ReflectNumericTraitsVector {
                                        component_count: 0,
                                    },
                                    matrix: ReflectNumericTraitsMatrix {
                                        column_count: 0,
                                        row_count: 0,
                                        stride: 0,
                                    },
                                },
                                image: ReflectImageTraits {
                                    dim: Type1d,
                                    depth: 0,
                                    arrayed: 0,
                                    ms: 0,
                                    sampled: 0,
                                    image_format: Undefined,
                                },
                                array: ReflectArrayTraits {
                                    dims: [
                                        0,
                                    ],
                                    stride: 4,
                                },
                            },
                            members: [],
                        },
                    ],
                },
            ),
            word_offset: (
                1334,
                1330,
            ),
            internal_data: 0x000001f660934f00,
        },
        ReflectDescriptorBinding {
            spirv_id: 872,
            name: "scene_tex",
            binding: 3,
            input_attachment_index: 0,
            set: 0,
            descriptor_type: StorageImage,
            resource_type: UnorderedAccessView,
            image: ReflectImageTraits {
                dim: Type3d,
                depth: 0,
                arrayed: 0,
                ms: 0,
                sampled: 2,
                image_format: RG32_UINT,
            },
            block: ReflectBlockVariable {
                spirv_id: 0,
                name: "",
                offset: 0,
                absolute_offset: 0,
                size: 0,
                padded_size: 0,
                decoration_flags: NONE,
                numeric: ReflectNumericTraits {
                    scalar: ReflectNumericTraitsScalar {
                        width: 0,
                        signedness: 0,
                    },
                    vector: ReflectNumericTraitsVector {
                        component_count: 0,
                    },
                    matrix: ReflectNumericTraitsMatrix {
                        column_count: 0,
                        row_count: 0,
                        stride: 0,
                    },
                },
                array: ReflectArrayTraits {
                    dims: [],
                    stride: 0,
                },
                members: [],
                type_description: None,
            },
            array: ReflectBindingArrayTraits {
                dims: [],
            },
            count: 1,
            uav_counter_id: 4294967295,
            uav_counter_binding: None,
            type_description: Some(
                ReflectTypeDescription {
                    id: 870,
                    op: ReflectOp(
                        TypeImage,
                    ),
                    type_name: "",
                    struct_member_name: "",
                    storage_class: Undefined,
                    type_flags: INT | EXTERNAL_IMAGE,
                    decoration_flags: NONE,
                    traits: ReflectTypeDescriptionTraits {
                        numeric: ReflectNumericTraits {
                            scalar: ReflectNumericTraitsScalar {
                                width: 32,
                                signedness: 0,
                            },
                            vector: ReflectNumericTraitsVector {
                                component_count: 0,
                            },
                            matrix: ReflectNumericTraitsMatrix {
                                column_count: 0,
                                row_count: 0,
                                stride: 0,
                            },
                        },
                        image: ReflectImageTraits {
                            dim: Type3d,
                            depth: 0,
                            arrayed: 0,
                            ms: 0,
                            sampled: 2,
                            image_format: RG32_UINT,
                        },
                        array: ReflectArrayTraits {
                            dims: [],
                            stride: 0,
                        },
                    },
                    members: [],
                },
            ),
            word_offset: (
                1342,
                1338,
            ),
            internal_data: 0x000001f660935168,
        },
        ReflectDescriptorBinding {
            spirv_id: 1263,
            name: "player_collision_result",
            binding: 4,
            input_attachment_index: 0,
            set: 0,
            descriptor_type: StorageBuffer,
            resource_type: UnorderedAccessView,
            image: ReflectImageTraits {
                dim: Type1d,
                depth: 0,
                arrayed: 0,
                ms: 0,
                sampled: 0,
                image_format: Undefined,
            },
            block: ReflectBlockVariable {
                spirv_id: 0,
                name: "player_collision_result",
                offset: 0,
                absolute_offset: 0,
                size: 0,
                padded_size: 0,
                decoration_flags: NONE,
                numeric: ReflectNumericTraits {
                    scalar: ReflectNumericTraitsScalar {
                        width: 0,
                        signedness: 0,
                    },
                    vector: ReflectNumericTraitsVector {
                        component_count: 0,
                    },
                    matrix: ReflectNumericTraitsMatrix {
                        column_count: 0,
                        row_count: 0,
                        stride: 0,
                    },
                },
                array: ReflectArrayTraits {
                    dims: [],
                    stride: 0,
                },
                members: [
                    ReflectBlockVariable {
                        spirv_id: 0,
                        name: "ground_distance",
                        offset: 0,
                        absolute_offset: 0,
                        size: 4,
                        padded_size: 4,
                        decoration_flags: NON_READABLE,
                        numeric: ReflectNumericTraits {
                            scalar: ReflectNumericTraitsScalar {
                                width: 32,
                                signedness: 0,
                            },
                            vector: ReflectNumericTraitsVector {
                                component_count: 0,
                            },
                            matrix: ReflectNumericTraitsMatrix {
                                column_count: 0,
                                row_count: 0,
                                stride: 0,
                            },
                        },
                        array: ReflectArrayTraits {
                            dims: [],
                            stride: 0,
                        },
                        members: [],
                        type_description: Some(
                            ReflectTypeDescription {
                                id: 6,
                                op: ReflectOp(
                                    TypeFloat,
                                ),
                                type_name: "",
                                struct_member_name: "ground_distance",
                                storage_class: UniformConstant,
                                type_flags: FLOAT,
                                decoration_flags: NONE,
                                traits: ReflectTypeDescriptionTraits {
                                    numeric: ReflectNumericTraits {
                                        scalar: ReflectNumericTraitsScalar {
                                            width: 32,
                                            signedness: 0,
                                        },
                                        vector: ReflectNumericTraitsVector {
                                            component_count: 0,
                                        },
                                        matrix: ReflectNumericTraitsMatrix {
                                            column_count: 0,
                                            row_count: 0,
                                            stride: 0,
                                        },
                                    },
                                    image: ReflectImageTraits {
                                        dim: Type1d,
                                        depth: 0,
                                        arrayed: 0,
                                        ms: 0,
                                        sampled: 0,
                                        image_format: Undefined,
                                    },
                                    array: ReflectArrayTraits {
                                        dims: [],
                                        stride: 0,
                                    },
                                },
                                members: [],
                            },
                        ),
                    },
                    ReflectBlockVariable {
                        spirv_id: 0,
                        name: "ring_distances",
                        offset: 4,
                        absolute_offset: 4,
                        size: 128,
                        padded_size: 128,
                        decoration_flags: NON_READABLE,
                        numeric: ReflectNumericTraits {
                            scalar: ReflectNumericTraitsScalar {
                                width: 32,
                                signedness: 0,
                            },
                            vector: ReflectNumericTraitsVector {
                                component_count: 0,
                            },
                            matrix: ReflectNumericTraitsMatrix {
                                column_count: 0,
                                row_count: 0,
                                stride: 0,
                            },
                        },
                        array: ReflectArrayTraits {
                            dims: [
                                32,
                            ],
                            stride: 4,
                        },
                        members: [],
                        type_description: Some(
                            ReflectTypeDescription {
                                id: 1260,
                                op: ReflectOp(
                                    TypeArray,
                                ),
                                type_name: "",
                                struct_member_name: "ring_distances",
                                storage_class: UniformConstant,
                                type_flags: FLOAT | ARRAY,
                                decoration_flags: NONE,
                                traits: ReflectTypeDescriptionTraits {
                                    numeric: ReflectNumericTraits {
                                        scalar: ReflectNumericTraitsScalar {
                                            width: 32,
                                            signedness: 0,
                                        },
                                        vector: ReflectNumericTraitsVector {
                                            component_count: 0,
                                        },
                                        matrix: ReflectNumericTraitsMatrix {
                                            column_count: 0,
                                            row_count: 0,
                                            stride: 0,
                                        },
                                    },
                                    image: ReflectImageTraits {
                                        dim: Type1d,
                                        depth: 0,
                                        arrayed: 0,
                                        ms: 0,
                                        sampled: 0,
                                        image_format: Undefined,
                                    },
                                    array: ReflectArrayTraits {
                                        dims: [
                                            32,
                                        ],
                                        stride: 4,
                                    },
                                },
                                members: [],
                            },
                        ),
                    },
                ],
                type_description: Some(
                    ReflectTypeDescription {
                        id: 1261,
                        op: ReflectOp(
                            TypeStruct,
                        ),
                        type_name: "B_PlayerCollisionResult",
                        struct_member_name: "",
                        storage_class: Undefined,
                        type_flags: EXTERNAL_BLOCK | STRUCT,
                        decoration_flags: BLOCK,
                        traits: ReflectTypeDescriptionTraits {
                            numeric: ReflectNumericTraits {
                                scalar: ReflectNumericTraitsScalar {
                                    width: 0,
                                    signedness: 0,
                                },
                                vector: ReflectNumericTraitsVector {
                                    component_count: 0,
                                },
                                matrix: ReflectNumericTraitsMatrix {
                                    column_count: 0,
                                    row_count: 0,
                                    stride: 0,
                                },
                            },
                            image: ReflectImageTraits {
                                dim: Type1d,
                                depth: 0,
                                arrayed: 0,
                                ms: 0,
                                sampled: 0,
                                image_format: Undefined,
                            },
                            array: ReflectArrayTraits {
                                dims: [],
                                stride: 0,
                            },
                        },
                        members: [
                            ReflectTypeDescription {
                                id: 6,
                                op: ReflectOp(
                                    TypeFloat,
                                ),
                                type_name: "",
                                struct_member_name: "ground_distance",
                                storage_class: UniformConstant,
                                type_flags: FLOAT,
                                decoration_flags: NONE,
                                traits: ReflectTypeDescriptionTraits {
                                    numeric: ReflectNumericTraits {
                                        scalar: ReflectNumericTraitsScalar {
                                            width: 32,
                                            signedness: 0,
                                        },
                                        vector: ReflectNumericTraitsVector {
                                            component_count: 0,
                                        },
                                        matrix: ReflectNumericTraitsMatrix {
                                            column_count: 0,
                                            row_count: 0,
                                            stride: 0,
                                        },
                                    },
                                    image: ReflectImageTraits {
                                        dim: Type1d,
                                        depth: 0,
                                        arrayed: 0,
                                        ms: 0,
                                        sampled: 0,
                                        image_format: Undefined,
                                    },
                                    array: ReflectArrayTraits {
                                        dims: [],
                                        stride: 0,
                                    },
                                },
                                members: [],
                            },
                            ReflectTypeDescription {
                                id: 1260,
                                op: ReflectOp(
                                    TypeArray,
                                ),
                                type_name: "",
                                struct_member_name: "ring_distances",
                                storage_class: UniformConstant,
                                type_flags: FLOAT | ARRAY,
                                decoration_flags: NONE,
                                traits: ReflectTypeDescriptionTraits {
                                    numeric: ReflectNumericTraits {
                                        scalar: ReflectNumericTraitsScalar {
                                            width: 32,
                                            signedness: 0,
                                        },
                                        vector: ReflectNumericTraitsVector {
                                            component_count: 0,
                                        },
                                        matrix: ReflectNumericTraitsMatrix {
                                            column_count: 0,
                                            row_count: 0,
                                            stride: 0,
                                        },
                                    },
                                    image: ReflectImageTraits {
                                        dim: Type1d,
                                        depth: 0,
                                        arrayed: 0,
                                        ms: 0,
                                        sampled: 0,
                                        image_format: Undefined,
                                    },
                                    array: ReflectArrayTraits {
                                        dims: [
                                            32,
                                        ],
                                        stride: 4,
                                    },
                                },
                                members: [],
                            },
                        ],
                    },
                ),
            },
            array: ReflectBindingArrayTraits {
                dims: [],
            },
            count: 1,
            uav_counter_id: 4294967295,
            uav_counter_binding: None,
            type_description: Some(
                ReflectTypeDescription {
                    id: 1261,
                    op: ReflectOp(
                        TypeStruct,
                    ),
                    type_name: "B_PlayerCollisionResult",
                    struct_member_name: "",
                    storage_class: Undefined,
                    type_flags: EXTERNAL_BLOCK | STRUCT,
                    decoration_flags: BLOCK,
                    traits: ReflectTypeDescriptionTraits {
                        numeric: ReflectNumericTraits {
                            scalar: ReflectNumericTraitsScalar {
                                width: 0,
                                signedness: 0,
                            },
                            vector: ReflectNumericTraitsVector {
                                component_count: 0,
                            },
                            matrix: ReflectNumericTraitsMatrix {
                                column_count: 0,
                                row_count: 0,
                                stride: 0,
                            },
                        },
                        image: ReflectImageTraits {
                            dim: Type1d,
                            depth: 0,
                            arrayed: 0,
                            ms: 0,
                            sampled: 0,
                            image_format: Undefined,
                        },
                        array: ReflectArrayTraits {
                            dims: [],
                            stride: 0,
                        },
                    },
                    members: [
                        ReflectTypeDescription {
                            id: 6,
                            op: ReflectOp(
                                TypeFloat,
                            ),
                            type_name: "",
                            struct_member_name: "ground_distance",
                            storage_class: UniformConstant,
                            type_flags: FLOAT,
                            decoration_flags: NONE,
                            traits: ReflectTypeDescriptionTraits {
                                numeric: ReflectNumericTraits {
                                    scalar: ReflectNumericTraitsScalar {
                                        width: 32,
                                        signedness: 0,
                                    },
                                    vector: ReflectNumericTraitsVector {
                                        component_count: 0,
                                    },
                                    matrix: ReflectNumericTraitsMatrix {
                                        column_count: 0,
                                        row_count: 0,
                                        stride: 0,
                                    },
                                },
                                image: ReflectImageTraits {
                                    dim: Type1d,
                                    depth: 0,
                                    arrayed: 0,
                                    ms: 0,
                                    sampled: 0,
                                    image_format: Undefined,
                                },
                                array: ReflectArrayTraits {
                                    dims: [],
                                    stride: 0,
                                },
                            },
                            members: [],
                        },
                        ReflectTypeDescription {
                            id: 1260,
                            op: ReflectOp(
                                TypeArray,
                            ),
                            type_name: "",
                            struct_member_name: "ring_distances",
                            storage_class: UniformConstant,
                            type_flags: FLOAT | ARRAY,
                            decoration_flags: NONE,
                            traits: ReflectTypeDescriptionTraits {
                                numeric: ReflectNumericTraits {
                                    scalar: ReflectNumericTraitsScalar {
                                        width: 32,
                                        signedness: 0,
                                    },
                                    vector: ReflectNumericTraitsVector {
                                        component_count: 0,
                                    },
                                    matrix: ReflectNumericTraitsMatrix {
                                        column_count: 0,
                                        row_count: 0,
                                        stride: 0,
                                    },
                                },
                                image: ReflectImageTraits {
                                    dim: Type1d,
                                    depth: 0,
                                    arrayed: 0,
                                    ms: 0,
                                    sampled: 0,
                                    image_format: Undefined,
                                },
                                array: ReflectArrayTraits {
                                    dims: [
                                        32,
                                    ],
                                    stride: 4,
                                },
                            },
                            members: [],
                        },
                    ],
                },
            ),
            word_offset: (
                1403,
                1399,
            ),
            internal_data: 0x000001f6609353d0,
        },
    ],
}

this is the content of
log::debug!("player_collider_sm: {:#?}", player_collider_sm);
inside tracer/mod.rs

currently the ShaderModule struct can extract these information for this sample shader, you can find the shader file yourself (player_collider.comp).
now i want you to extend the ShaderModule struct, so it can extract the information for this shader.

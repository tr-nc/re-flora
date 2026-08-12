use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Data, DeriveInput, Field, Fields, GenericArgument, PathArguments, Type, TypePath,
    parse_macro_input,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResourceKind {
    Buffer,
    Texture,
    AccelerationStructure,
}

#[proc_macro_derive(ResourceContainer, attributes(resource))]
pub fn derive_resource_container(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = input.ident;

    let fields = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(named) => named.named,
            _ => {
                return syn::Error::new_spanned(
                    struct_name,
                    "ResourceContainer can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(
                struct_name,
                "ResourceContainer can only be derived for structs",
            )
            .to_compile_error()
            .into();
        }
    };

    let mut resources = Vec::new();
    let mut nested_containers = Vec::new();
    for field in fields {
        let Some(ident) = field.ident.clone() else {
            continue;
        };
        let kind = match resource_kind(&field.ty) {
            Ok(kind) => kind,
            Err(error) => return error.to_compile_error().into(),
        };
        let nested = match is_nested(&field) {
            Ok(nested) => nested,
            Err(error) => return error.to_compile_error().into(),
        };
        match (kind, nested) {
            (Some(_), true) => {
                return syn::Error::new_spanned(
                    field,
                    "Resource<T> fields are providers already and cannot be marked nested",
                )
                .to_compile_error()
                .into();
            }
            (Some(kind), false) => resources.push((ident, kind)),
            (None, true) => nested_containers.push(ident),
            (None, false) => {}
        }
    }

    if resources.is_empty() && nested_containers.is_empty() {
        return syn::Error::new_spanned(
            struct_name,
            "no Resource<T> or #[resource(nested)] fields found; cannot derive ResourceContainer",
        )
        .to_compile_error()
        .into();
    }

    let direct_match_arms = resources.iter().map(|(ident, kind)| {
        let variant = match kind {
            ResourceKind::Buffer => quote! { Buffer },
            ResourceKind::Texture => quote! { Texture },
            ResourceKind::AccelerationStructure => quote! { AccelerationStructure },
        };
        quote! {
            stringify!(#ident) => re_flora_vkn::ResourceLookup::Unique(
                re_flora_vkn::DescriptorResource::#variant(&self.#ident),
            ),
        }
    });

    let expanded = quote! {
        impl re_flora_vkn::ResourceContainer for #struct_name {
            fn resolve_resource(&self, name: &str) -> re_flora_vkn::ResourceLookup<'_> {
                let mut lookup = match name {
                    #(#direct_match_arms)*
                    _ => re_flora_vkn::ResourceLookup::Missing,
                };
                #(
                    lookup = lookup.merge(
                        re_flora_vkn::ResourceContainer::resolve_resource(
                            &self.#nested_containers,
                            name,
                        ),
                    );
                )*
                lookup
            }
        }
    };
    TokenStream::from(expanded)
}

fn resource_kind(ty: &Type) -> syn::Result<Option<ResourceKind>> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return Ok(None);
    };
    let Some(resource_segment) = path.segments.last() else {
        return Ok(None);
    };
    if resource_segment.ident != "Resource" {
        return Ok(None);
    }
    let PathArguments::AngleBracketed(arguments) = &resource_segment.arguments else {
        return Err(syn::Error::new_spanned(
            ty,
            "Resource must have one Buffer, Texture, or AccelStruct type argument",
        ));
    };
    let mut type_arguments = arguments.args.iter().filter_map(|argument| match argument {
        GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    let Some(resource_type) = type_arguments.next() else {
        return Err(syn::Error::new_spanned(
            ty,
            "Resource must have one Buffer, Texture, or AccelStruct type argument",
        ));
    };
    if type_arguments.next().is_some() || arguments.args.len() != 1 {
        return Err(syn::Error::new_spanned(
            ty,
            "Resource must have exactly one type argument",
        ));
    }
    let Type::Path(TypePath { path, .. }) = resource_type else {
        return Err(syn::Error::new_spanned(
            resource_type,
            "unsupported Resource type; expected Buffer, Texture, or AccelStruct",
        ));
    };
    let kind = match path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
    {
        Some(name) if name == "Buffer" => ResourceKind::Buffer,
        Some(name) if name == "Texture" => ResourceKind::Texture,
        Some(name) if name == "AccelStruct" => ResourceKind::AccelerationStructure,
        _ => {
            return Err(syn::Error::new_spanned(
                resource_type,
                "unsupported Resource type; expected Buffer, Texture, or AccelStruct",
            ));
        }
    };
    Ok(Some(kind))
}

fn is_nested(field: &Field) -> syn::Result<bool> {
    let mut nested = false;
    for attribute in &field.attrs {
        if !attribute.path().is_ident("resource") {
            continue;
        }
        attribute.parse_nested_meta(|meta| {
            if !meta.path.is_ident("nested") {
                return Err(meta.error("expected `nested`"));
            }
            if nested {
                return Err(meta.error("duplicate `nested` marker"));
            }
            nested = true;
            Ok(())
        })?;
    }
    Ok(nested)
}

#[cfg(test)]
mod tests {
    use super::{ResourceKind, is_nested, resource_kind};
    use syn::{Field, Type, parse_quote};

    #[test]
    fn classifies_supported_resource_types_statically() {
        let buffer: Type = parse_quote!(Resource<Buffer>);
        let texture: Type = parse_quote!(crate::Resource<re_flora_vkn::Texture>);
        let acceleration_structure: Type = parse_quote!(Resource<AccelStruct>);

        assert_eq!(resource_kind(&buffer).unwrap(), Some(ResourceKind::Buffer));
        assert_eq!(
            resource_kind(&texture).unwrap(),
            Some(ResourceKind::Texture)
        );
        assert_eq!(
            resource_kind(&acceleration_structure).unwrap(),
            Some(ResourceKind::AccelerationStructure)
        );
    }

    #[test]
    fn rejects_unsupported_resource_types() {
        let unsupported: Type = parse_quote!(Resource<Sampler>);
        let error = resource_kind(&unsupported).unwrap_err().to_string();
        assert!(error.contains("Buffer, Texture, or AccelStruct"));
    }

    #[test]
    fn nested_containers_are_explicit() {
        let nested: Field = parse_quote!(#[resource(nested)] resources: Resources);
        let ordinary: Field = parse_quote!(meshes: Meshes);

        assert!(is_nested(&nested).unwrap());
        assert!(!is_nested(&ordinary).unwrap());
    }
}

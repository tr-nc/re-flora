use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, PathArguments, Type, TypePath, parse_macro_input};

#[proc_macro_derive(ResourceContainer)]
pub fn derive_resource_container(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = input.ident;

    // we only support structs with named fields
    let fields = match input.data {
        Data::Struct(s) => match s.fields {
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

    // collect different types of fields
    let mut resource_idents = Vec::<syn::Ident>::new();
    let mut other_field_idents = Vec::<syn::Ident>::new();
    let mut other_field_types = Vec::<Type>::new();
    
    for field in fields {
        if let Some(ident) = &field.ident {
            if is_resource_type(&field.ty) {
                resource_idents.push(ident.clone());
            } else if is_potential_resource_container(&field.ty) {
                // Only include types that could potentially be ResourceContainer implementors
                other_field_idents.push(ident.clone());
                other_field_types.push(field.ty.clone());
            }
            // Skip primitive types, standard library types, etc.
        }
    }

    // Check if we have at least one field that could be a resource
    if resource_idents.is_empty() && other_field_idents.is_empty() {
        return syn::Error::new_spanned(
            struct_name,
            "no Resource<T> fields found; cannot derive ResourceContainer",
        )
        .to_compile_error()
        .into();
    }

    // generate match arms for direct Resource<Buffer> fields
    let buffer_match_arms = resource_idents.iter().map(|ident| {
        quote! {
            stringify!(#ident) => self.#ident.as_any().downcast_ref::<crate::vkn::Buffer>(),
        }
    });

    // generate match arms for direct Resource<Texture> fields  
    let texture_match_arms = resource_idents.iter().map(|ident| {
        quote! {
            stringify!(#ident) => self.#ident.as_any().downcast_ref::<crate::vkn::Texture>(),
        }
    });

    // generate nested lookup code for buffers
    let nested_buffer_lookup_code = if other_field_idents.is_empty() {
        quote! {}
    } else {
        quote! {
            // Try nested ResourceContainer fields recursively
            #(
                if let Some(result) = self.#other_field_idents.get_buffer(name) {
                    return Some(result);
                }
            )*
        }
    };

    // generate nested lookup code for textures
    let nested_texture_lookup_code = if other_field_idents.is_empty() {
        quote! {}
    } else {
        quote! {
            // Try nested ResourceContainer fields recursively
            #(
                if let Some(result) = self.#other_field_idents.get_texture(name) {
                    return Some(result);
                }
            )*
        }
    };

    // generate resource names for conflict detection
    let direct_resource_names = resource_idents.iter().map(|ident| {
        quote! { stringify!(#ident) }
    });

    let _nested_resource_names = other_field_types.iter().map(|ty| {
        quote! { #ty::get_resource_names() }
    });

    // generate compile-time conflict detection
    let direct_names_array = if resource_idents.is_empty() {
        quote! { &[] }
    } else {
        let names = resource_idents.iter().map(|ident| {
            quote! { stringify!(#ident) }
        });
        quote! { &[#(#names),*] }
    };

    // Runtime conflict detection (since we removed the const)
    let runtime_checks = if !other_field_types.is_empty() {
        quote! {
            // Runtime checks for name conflicts
            let direct_names = #direct_names_array;
            #(
                let nested_names = self.#other_field_idents.get_resource_names();
                for direct_name in direct_names {
                    for nested_name in &nested_names {
                        if direct_name == nested_name {
                            panic!("Resource name conflict detected: '{}'", direct_name);
                        }
                    }
                }
            )*
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        impl crate::resource::ResourceContainer for #struct_name {
            fn get_buffer(&self, name: &str) -> Option<&crate::vkn::Buffer> {
                #runtime_checks
                match name {
                    // Direct Resource<Buffer> fields take priority
                    #(#buffer_match_arms)*
                    _ => {
                        // Try nested ResourceContainer fields
                        #nested_buffer_lookup_code
                        None
                    }
                }
            }

            fn get_texture(&self, name: &str) -> Option<&crate::vkn::Texture> {
                match name {
                    // Direct Resource<Texture> fields take priority
                    #(#texture_match_arms)*
                    _ => {
                        // Try nested ResourceContainer fields
                        #nested_texture_lookup_code
                        None
                    }
                }
            }

            fn get_resource_names(&self) -> Vec<&'static str> {
                let mut names = Vec::new();
                let mut seen = std::collections::HashSet::new();
                
                // Add direct resource names
                #(
                    let name = #direct_resource_names;
                    if !seen.insert(name) {
                        panic!("Duplicate resource name '{}' found in {}", name, stringify!(#struct_name));
                    }
                    names.push(name);
                )*
                
                // Add nested resource names
                #(
                    for nested_name in self.#other_field_idents.get_resource_names() {
                        if !seen.insert(nested_name) {
                            panic!("Duplicate resource name '{}' found in {} (conflicts with nested resource)", nested_name, stringify!(#struct_name));
                        }
                        names.push(nested_name);
                    }
                )*
                
                names
            }
        }
    };
    TokenStream::from(expanded)
}

/// returns true if the type is exactly Resource<...>
fn is_resource_type(ty: &Type) -> bool {
    match ty {
        Type::Path(TypePath { path, .. }) => {
            if let Some(last) = path.segments.last() {
                last.ident == "Resource"
                    && matches!(last.arguments, PathArguments::AngleBracketed(_))
            } else {
                false
            }
        }
        _ => false,
    }
}

/// returns true if the type could potentially be a ResourceContainer implementor
/// excludes obvious non-ResourceContainer types but doesn't make assumptions
fn is_potential_resource_container(ty: &Type) -> bool {
    match ty {
        Type::Path(TypePath { path, .. }) => {
            if let Some(last) = path.segments.last() {
                let type_name = last.ident.to_string();
                // Exclude primitive types and common standard library types
                !matches!(type_name.as_str(), 
                    "u8" | "u16" | "u32" | "u64" | "usize" |
                    "i8" | "i16" | "i32" | "i64" | "isize" |
                    "f32" | "f64" | "bool" | "char" |
                    "String" | "Vec" | "HashMap" | "HashSet" |
                    "Option" | "Result" | "Arc" | "Rc" | "Box" |
                    "Device" | "Allocator" | // Known VKN types that don't implement ResourceContainer
                    "Texture" | "Buffer" | "CommandBuffer" | "Pipeline" | // VKN types that are resources, not containers
                    "ShaderModule" | "DescriptorSet" | "RenderPass" | // More VKN types
                    "Context" | "Queue" | "Surface" | "Instance" | "PhysicalDevice" // VKN context types
                )
            } else {
                false
            }
        }
        _ => false,
    }
}


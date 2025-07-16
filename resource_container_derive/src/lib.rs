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

    // generate match arms for direct Resource<T> fields
    let direct_match_arms = resource_idents.iter().map(|ident| {
        quote! {
            stringify!(#ident) => self.#ident.as_any().downcast_ref::<T>(),
        }
    });

    // generate nested lookup code for other fields (let compiler determine if they implement ResourceContainer)
    let nested_lookup_code = if other_field_idents.is_empty() {
        quote! {}
    } else {
        quote! {
            #(
                if let Some(result) = self.#other_field_idents.get_resource::<T>(name) {
                    return Some(result);
                }
            )*
        }
    };

    // generate resource names for conflict detection
    let direct_resource_names = resource_idents.iter().map(|ident| {
        quote! { stringify!(#ident) }
    });

    let nested_resource_names = other_field_types.iter().map(|ty| {
        quote! { #ty::get_resource_names() }
    });

    let expanded = quote! {
        impl crate::resource::ResourceContainer for #struct_name {
            fn get_resource<T: 'static>(&self, name: &str) -> Option<&T> {
                match name {
                    // Direct Resource<T> fields take priority
                    #(#direct_match_arms)*
                    _ => {
                        // Try nested ResourceContainer fields
                        #nested_lookup_code
                        None
                    }
                }
            }

            fn get_resource_names() -> Vec<&'static str> {
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
                    for nested_name in #nested_resource_names {
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
                    "DenoiserTextureSet" // Plain struct with textures, not a ResourceContainer
                )
            } else {
                false
            }
        }
        _ => false,
    }
}


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

    // pick only fields of type Resource<...>
    let mut resource_idents = Vec::<syn::Ident>::new();
    for field in fields {
        if let Some(ident) = &field.ident {
            if is_resource_type(&field.ty) {
                resource_idents.push(ident.clone());
            }
        }
    }

    if resource_idents.is_empty() {
        return syn::Error::new_spanned(
            struct_name,
            "no Resource<T> fields found; cannot derive ResourceContainer",
        )
        .to_compile_error()
        .into();
    }

    let match_arms = resource_idents.iter().map(|ident| {
        quote! {
            stringify!(#ident) => self.#ident.as_any().downcast_ref::<T>(),
        }
    });

    let expanded = quote! {
        impl crate::resource::ResourceContainer for #struct_name {
            fn get_resource<T: 'static>(&self, name: &str) -> Option<&T> {
                match name {
                    #(#match_arms)*
                    _ => None,
                }
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

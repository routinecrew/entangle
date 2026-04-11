extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

/// Derive macro for `ZeroCopySafe`.
///
/// Validates at compile time that the type is safe for shared memory:
/// - Must have `#[repr(C)]` or `#[repr(transparent)]`
/// - All fields must implement `ZeroCopySafe`
/// - Generic type parameters get `ZeroCopySafe` bounds added automatically
///
/// # Example
/// ```ignore
/// #[derive(ZeroCopySafe)]
/// #[repr(C)]
/// struct SensorData {
///     timestamp: u64,
///     value: f64,
/// }
/// ```
#[proc_macro_derive(ZeroCopySafe)]
pub fn derive_zero_copy_safe(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_zero_copy_safe(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn impl_zero_copy_safe(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;

    // Check for #[repr(C)] or #[repr(transparent)]
    if !has_valid_repr(input) {
        return Err(syn::Error::new_spanned(
            name,
            "ZeroCopySafe requires #[repr(C)] or #[repr(transparent)]",
        ));
    }

    // Only structs are supported (not enums with data or unions)
    match &input.data {
        Data::Struct(data) => validate_struct_fields(&data.fields)?,
        Data::Enum(_) => {
            // Allow fieldless enums with repr(C) or repr(u*)
            if !is_fieldless_enum(&input.data) {
                return Err(syn::Error::new_spanned(
                    name,
                    "ZeroCopySafe enums must be fieldless (no data variants)",
                ));
            }
        }
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                name,
                "ZeroCopySafe cannot be derived for unions",
            ));
        }
    }

    // Build where clause: add ZeroCopySafe bound to all type parameters
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut where_predicates = Vec::new();
    if let Some(wc) = where_clause {
        for pred in &wc.predicates {
            where_predicates.push(quote! { #pred });
        }
    }
    for param in &input.generics.params {
        if let syn::GenericParam::Type(tp) = param {
            let ident = &tp.ident;
            where_predicates.push(quote! { #ident: ZeroCopySafe });
        }
    }

    let where_clause_tokens = if where_predicates.is_empty() {
        quote! {}
    } else {
        quote! { where #(#where_predicates),* }
    };

    // Generate field-level assertions for structs
    let field_assertions = generate_field_assertions(&input.data);

    Ok(quote! {
        // Safety: validated by the derive macro:
        // - #[repr(C)] or #[repr(transparent)] is present
        // - All fields implement ZeroCopySafe (enforced by const assertions below)
        unsafe impl #impl_generics ZeroCopySafe for #name #ty_generics #where_clause_tokens {}

        #[doc(hidden)]
        #[allow(non_snake_case)]
        const _: () = {
            // Compile-time assertion that all field types implement ZeroCopySafe
            fn _assert_zero_copy_safe<T: ZeroCopySafe>() {}
            fn _assert_fields #impl_generics () #where_clause_tokens {
                #field_assertions
            }
        };
    })
}

fn has_valid_repr(input: &DeriveInput) -> bool {
    for attr in &input.attrs {
        if attr.path().is_ident("repr") {
            if let Ok(nested) = attr.parse_args::<syn::Ident>() {
                let repr = nested.to_string();
                if repr == "C" || repr == "transparent" {
                    return true;
                }
                // Allow repr(u8), repr(u16), etc. for enums
                if repr.starts_with('u') || repr.starts_with('i') {
                    return true;
                }
            }
        }
    }
    false
}

fn is_fieldless_enum(data: &Data) -> bool {
    match data {
        Data::Enum(e) => e.variants.iter().all(|v| v.fields.is_empty()),
        _ => false,
    }
}

fn validate_struct_fields(_fields: &Fields) -> syn::Result<()> {
    // We don't reject specific types here — the const assertion in the
    // generated code will catch non-ZeroCopySafe fields at compile time.
    // This function is a placeholder for additional checks if needed.
    Ok(())
}

fn generate_field_assertions(data: &Data) -> proc_macro2::TokenStream {
    match data {
        Data::Struct(s) => {
            let assertions: Vec<_> = s
                .fields
                .iter()
                .map(|f| {
                    let ty = &f.ty;
                    quote! { _assert_zero_copy_safe::<#ty>(); }
                })
                .collect();
            quote! { #(#assertions)* }
        }
        _ => quote! {},
    }
}

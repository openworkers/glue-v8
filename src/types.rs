//! Type detection and extraction helpers.

use quote::quote;
use syn::Type;

/// Check if a type is a V8 Local type (e.g., v8::Local<v8::Function>)
/// Returns the inner type name if it is (e.g., "Function", "Value", "Object")
pub fn get_v8_local_inner_type(ty: &Type) -> Option<String> {
    if let Type::Path(type_path) = ty {
        let segments: Vec<_> = type_path.path.segments.iter().collect();

        // Check for v8::Local<T> or Local<T>
        let local_segment =
            if segments.len() == 2 && segments[0].ident == "v8" && segments[1].ident == "Local" {
                Some(&segments[1])
            } else if segments.len() == 1 && segments[0].ident == "Local" {
                Some(&segments[0])
            } else {
                None
            };

        if let Some(segment) = local_segment {
            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                if let Some(syn::GenericArgument::Type(Type::Path(inner_path))) = args.args.first()
                {
                    // Get the inner type name (e.g., v8::Function -> Function)
                    if let Some(last_segment) = inner_path.path.segments.last() {
                        return Some(last_segment.ident.to_string());
                    }
                }
            }
        }
    }

    None
}

/// Check if the return type is Result<T, E>
pub fn is_result_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Result";
        }
    }
    false
}

/// Check if type is Option<T> and return the inner type
pub fn get_option_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}

/// Check if type is Rc<T> and return the inner type
pub fn get_rc_inner_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Rc" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}

/// Generate extraction code for a V8 Local type with type check
pub fn v8_local_extraction(
    name: &syn::Ident,
    idx: i32,
    v8_type: &str,
    check_method: &str,
) -> proc_macro2::TokenStream {
    let v8_type_ident = syn::Ident::new(v8_type, name.span());
    let check_ident = syn::Ident::new(check_method, name.span());
    let error_msg = format!("argument {} must be a {}", idx, v8_type);

    quote! {
        let __v8g_tmp = args.get(#idx);
        if !__v8g_tmp.#check_ident() {
            let msg = v8::String::new(scope, #error_msg).unwrap();
            let err = v8::Exception::type_error(scope, msg);
            scope.throw_exception(err);
            return;
        }
        let #name: v8::Local<v8::#v8_type_ident> = __v8g_tmp.try_into().unwrap();
    }
}

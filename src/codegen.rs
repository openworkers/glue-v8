//! Code generation for V8 callback wrappers.

use quote::quote;
use syn::Type;

use crate::types::{
    get_option_inner_type, get_rc_inner_type, get_v8_local_inner_type, v8_local_extraction,
};

/// Generate state extraction code for the slow path.
///
/// Uses V8 context slots to store and retrieve state.
pub fn generate_state_extraction(
    has_state: bool,
    state_type: &Option<Type>,
) -> proc_macro2::TokenStream {
    if !has_state {
        return quote! {};
    }

    if let Some(state_ty) = state_type {
        let state_ty_str = quote!(#state_ty).to_string();

        // V8 Context::get_slot<T>() returns Option<Rc<T>>.
        // So if state_ty is Rc<Counter>, we need to call get_slot::<Counter>()
        // to get Option<Rc<Counter>>.
        if let Some(inner_ty) = get_rc_inner_type(state_ty) {
            quote! {
                let Some(state) = scope.get_current_context().get_slot::<#inner_ty>() else {
                    let msg = v8::String::new(scope, concat!("internal error: state not found for ", #state_ty_str)).unwrap();
                    let err = v8::Exception::error(scope, msg);
                    scope.throw_exception(err);
                    return;
                };
            }
        } else {
            // State type is not Rc<T>, try to use it directly
            // (this might not work with V8's slot API, but let's try)
            quote! {
                let Some(state) = scope.get_current_context().get_slot::<#state_ty>() else {
                    let msg = v8::String::new(scope, concat!("internal error: state not found for ", #state_ty_str)).unwrap();
                    let err = v8::Exception::error(scope, msg);
                    scope.throw_exception(err);
                    return;
                };
            }
        }
    } else {
        quote! {
            compile_error!("Function has 'state' parameter but no state type specified. Use #[glue_v8::method(state = YourStateType)]");
        }
    }
}

/// Generate argument extraction code for the slow path.
///
/// Handles various types:
/// - Option<T>: None if undefined/null
/// - v8::Local<T>: Direct V8 type extraction
/// - Other types: serde_v8 deserialization
pub fn generate_arg_extractions(
    params: &[(syn::Ident, Box<Type>)],
) -> Vec<proc_macro2::TokenStream> {
    params
        .iter()
        .enumerate()
        .map(|(i, (name, ty))| {
            let idx = i as i32;

            // Check if this is an Option<T> type
            if let Some(inner_ty) = get_option_inner_type(ty) {
                // Optional parameter: None if undefined/null, Some(value) otherwise
                let inner_type_str = quote!(#inner_ty).to_string();
                let error_prefix = format!("argument {}: expected {}", idx, inner_type_str);

                quote! {
                    let #name: #ty = {
                        let __v8g_arg = args.get(#idx);
                        if __v8g_arg.is_undefined() || __v8g_arg.is_null() {
                            None
                        } else {
                            match serde_v8::from_v8_any(scope, __v8g_arg) {
                                Ok(v) => Some(v),
                                Err(e) => {
                                    let msg = v8::String::new(scope, &format!("{}: {}", #error_prefix, e)).unwrap();
                                    let err = v8::Exception::type_error(scope, msg);
                                    scope.throw_exception(err);
                                    return;
                                }
                            }
                        }
                    };
                }
            } else if let Some(inner_type) = get_v8_local_inner_type(ty) {
                // V8 Local type - generate direct extraction
                match inner_type.as_str() {
                    "Function" => v8_local_extraction(name, idx, "Function", "is_function"),
                    "Object" => v8_local_extraction(name, idx, "Object", "is_object"),
                    "Array" => v8_local_extraction(name, idx, "Array", "is_array"),
                    "Uint8Array" => v8_local_extraction(name, idx, "Uint8Array", "is_uint8_array"),
                    "ArrayBuffer" => {
                        v8_local_extraction(name, idx, "ArrayBuffer", "is_array_buffer")
                    }
                    "String" => v8_local_extraction(name, idx, "String", "is_string"),
                    "Number" => v8_local_extraction(name, idx, "Number", "is_number"),
                    "Value" => {
                        // No type check needed for Value
                        quote! {
                            let #name: v8::Local<v8::Value> = args.get(#idx);
                        }
                    }
                    _ => {
                        // For other V8 types, try generic conversion
                        let type_str = quote!(#ty).to_string();
                        let error_msg = format!("argument {}: expected {}", idx, type_str);

                        quote! {
                            let #name: #ty = match args.get(#idx).try_into() {
                                Ok(v) => v,
                                Err(_) => {
                                    let msg = v8::String::new(scope, #error_msg).unwrap();
                                    let err = v8::Exception::type_error(scope, msg);
                                    scope.throw_exception(err);
                                    return;
                                }
                            };
                        }
                    }
                }
            } else {
                // Use serde_v8 for regular types
                let type_str = quote!(#ty).to_string();
                let error_prefix = format!("argument {}: expected {}", idx, type_str);

                quote! {
                    let #name: #ty = match serde_v8::from_v8_any(scope, args.get(#idx)) {
                        Ok(v) => v,
                        Err(e) => {
                            let msg = v8::String::new(scope, &format!("{}: {}", #error_prefix, e)).unwrap();
                            let err = v8::Exception::type_error(scope, msg);
                            scope.throw_exception(err);
                            return;
                        }
                    };
                }
            }
        })
        .collect()
}

/// Generate the function call and return value handling code.
///
/// Handles:
/// - Promise mode: wrap in Promise, resolve/reject
/// - Result<T, E>: throw on Err, return Ok value
/// - Regular return: convert via serde_v8
/// - No return: just call
pub fn generate_call_and_return(
    fn_name: &syn::Ident,
    call_args: &[proc_macro2::TokenStream],
    has_return: bool,
    returns_result: bool,
    is_promise: bool,
) -> proc_macro2::TokenStream {
    if is_promise {
        // Promise mode: wrap in a Promise, handle Result<T, E> if applicable
        if returns_result {
            quote! {
                let resolver = v8::PromiseResolver::new(scope).unwrap();
                let promise = resolver.get_promise(scope);
                rv.set(promise.into());

                match #fn_name(#(#call_args),*) {
                    Ok(value) => {
                        if let Ok(v8_value) = serde_v8::to_v8(scope, value) {
                            resolver.resolve(scope, v8_value);
                        }
                    }
                    Err(err) => {
                        let err_str = format!("{}", err);
                        let msg = v8::String::new(scope, &err_str).unwrap();
                        let error = v8::Exception::error(scope, msg);
                        resolver.reject(scope, error);
                    }
                }
            }
        } else if has_return {
            // Promise mode but not Result - just resolve with value
            quote! {
                let resolver = v8::PromiseResolver::new(scope).unwrap();
                let promise = resolver.get_promise(scope);
                rv.set(promise.into());

                let result = #fn_name(#(#call_args),*);
                if let Ok(v8_value) = serde_v8::to_v8(scope, result) {
                    resolver.resolve(scope, v8_value);
                }
            }
        } else {
            // Promise mode, no return - resolve with undefined
            quote! {
                let resolver = v8::PromiseResolver::new(scope).unwrap();
                let promise = resolver.get_promise(scope);
                rv.set(promise.into());

                #fn_name(#(#call_args),*);
                resolver.resolve(scope, v8::undefined(scope).into());
            }
        }
    } else if returns_result {
        // Not promise mode but returns Result - throw on Err
        quote! {
            match #fn_name(#(#call_args),*) {
                Ok(value) => {
                    if let Ok(v8_value) = serde_v8::to_v8(scope, value) {
                        rv.set(v8_value);
                    }
                }
                Err(err) => {
                    let err_str = format!("{}", err);
                    let msg = v8::String::new(scope, &err_str).unwrap();
                    let error = v8::Exception::error(scope, msg);
                    scope.throw_exception(error);
                }
            }
        }
    } else if has_return {
        quote! {
            let result = #fn_name(#(#call_args),*);
            if let Ok(v8_result) = serde_v8::to_v8(scope, result) {
                rv.set(v8_result);
            }
        }
    } else {
        quote! {
            #fn_name(#(#call_args),*);
        }
    }
}

//! V8 Glue - Rust to V8 binding macros for OpenWorkers
//!
//! Generate V8 callback boilerplate from Rust functions.

use proc_macro::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, Pat, ReturnType, Type, parse_macro_input};

/// Check if a type is a V8 Local type (e.g., v8::Local<v8::Function>)
/// Returns the inner type name if it is (e.g., "Function", "Value", "Object")
fn get_v8_local_inner_type(ty: &Type) -> Option<String> {
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

/// Generate extraction code for a V8 Local type with type check
fn v8_local_extraction(
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

/// Parsed attributes for #[gv8::method]
struct MethodAttrs {
    js_name: Option<String>,
    state_type: Option<syn::Type>,
    promise: bool,
}

impl MethodAttrs {
    fn parse(attr: TokenStream) -> Self {
        let mut js_name = None;
        let mut state_type = None;
        let mut promise = false;

        if !attr.is_empty() {
            let parser = syn::meta::parser(|meta| {
                if meta.path.is_ident("state") {
                    let value: syn::Type = meta.value()?.parse()?;
                    state_type = Some(value);
                    Ok(())
                } else if meta.path.is_ident("name") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    js_name = Some(value.value());
                    Ok(())
                } else if meta.path.is_ident("promise") {
                    promise = true;
                    Ok(())
                } else {
                    Err(meta.error("expected `state = Type`, `name = \"jsName\"`, or `promise`"))
                }
            });

            // Try parsing as key-value pairs first
            if syn::parse::Parser::parse(parser, attr.clone()).is_err() {
                // Fall back to bare string literal
                if let Ok(lit) = syn::parse::<syn::LitStr>(attr) {
                    js_name = Some(lit.value());
                }
            }
        }

        Self {
            js_name,
            state_type,
            promise,
        }
    }
}

/// Check if the return type is Result<T, E>
fn is_result_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Result";
        }
    }
    false
}

/// Check if type is Option<T> and return the inner type
fn get_option_inner_type(ty: &Type) -> Option<&Type> {
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
fn get_rc_inner_type(ty: &Type) -> Option<&Type> {
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

/// Generate a V8 callback wrapper for a Rust function.
///
/// # Examples
///
/// Basic function (no state):
/// ```ignore
/// #[gv8::method]
/// fn add(scope: &mut v8::PinScope, a: f64, b: f64) -> f64 {
///     a + b
/// }
/// ```
///
/// With state from context slot:
/// ```ignore
/// #[gv8::method(state = TimerState)]
/// fn schedule_timeout(scope: &mut v8::PinScope, state: &TimerState, id: u64, delay: u64) {
///     let _ = state.scheduler_tx.send(SchedulerMessage::ScheduleTimeout(id, delay));
/// }
/// ```
///
/// With custom JS name:
/// ```ignore
/// #[gv8::method(name = "setTimeout")]
/// fn set_timeout(scope: &mut v8::PinScope, delay: f64) -> u64 { ... }
/// ```
///
/// With Result return type (throws on Err):
/// ```ignore
/// #[gv8::method]
/// fn parse_json(scope: &mut v8::PinScope, input: String) -> Result<serde_json::Value, String> {
///     serde_json::from_str(&input).map_err(|e| e.to_string())
/// }
/// ```
///
/// With Promise (returns JS Promise):
/// ```ignore
/// #[gv8::method(promise)]
/// fn fetch_data(scope: &mut v8::PinScope, url: String) -> Result<String, String> {
///     // Ok(value) → Promise.resolve(value)
///     // Err(msg)  → Promise.reject(new Error(msg))
///     Ok("data".to_string())
/// }
/// ```
///
/// With optional parameters:
/// ```ignore
/// #[gv8::method]
/// fn greet(scope: &mut v8::PinScope, name: String, title: Option<String>) -> String {
///     // greet("Alice") → title is None
///     // greet("Alice", "Dr.") → title is Some("Dr.")
///     match title {
///         Some(t) => format!("{} {}", t, name),
///         None => name,
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn method(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = MethodAttrs::parse(attr);
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let _js_name = attrs.js_name.unwrap_or_else(|| fn_name.to_string());
    let wrapper_name = syn::Ident::new(&format!("{}_v8", fn_name), fn_name.span());

    // Extract parameters, tracking which are special (scope, state)
    let mut has_scope = false;
    let mut has_state = false;
    let params: Vec<_> = input_fn
        .sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pat_type) = arg {
                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    let name = &pat_ident.ident;
                    let ty = &pat_type.ty;

                    // Skip 'scope' or '_scope' - provided by V8 callback
                    let name_str = name.to_string();
                    if name_str == "scope" || name_str == "_scope" {
                        has_scope = true;
                        return None;
                    }

                    // Skip 'state' - will be extracted from context slot
                    if name_str == "state" {
                        has_state = true;
                        return None;
                    }

                    return Some((name.clone(), ty.clone()));
                }
            }
            None
        })
        .collect();

    // Generate argument extraction code
    let arg_extractions: Vec<_> = params
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
                // Check if this is a V8 Local type
                // Generate direct V8 extraction
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
        .collect();

    // Generate state extraction if needed
    let state_extraction = if has_state {
        if let Some(state_ty) = &attrs.state_type {
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
                compile_error!("Function has 'state' parameter but no state type specified. Use #[gv8::method(state = YourStateType)]");
            }
        }
    } else {
        quote! {}
    };

    // Generate function call arguments
    let call_args: Vec<_> = {
        let mut args = Vec::new();
        if has_scope {
            args.push(quote! { scope });
        }
        if has_state {
            args.push(quote! { &state });
        }
        for (name, _) in &params {
            args.push(quote! { #name });
        }
        args
    };

    // Check if function has a return type and if it's a Result
    let has_return = !matches!(input_fn.sig.output, ReturnType::Default);
    let returns_result = if let ReturnType::Type(_, ty) = &input_fn.sig.output {
        is_result_type(ty)
    } else {
        false
    };

    let call_and_return = if attrs.promise {
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
    };

    let expanded = quote! {
        #input_fn

        /// V8 callback wrapper - auto-generated by gv8::method
        pub fn #wrapper_name(
            scope: &mut v8::PinScope,
            args: v8::FunctionCallbackArguments,
            mut rv: v8::ReturnValue,
        ) {
            #state_extraction
            #(#arg_extractions)*
            #call_and_return
        }
    };

    TokenStream::from(expanded)
}

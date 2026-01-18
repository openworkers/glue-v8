//! V8 Fast API code generation.
//!
//! Fast API enables ~10x faster function calls for hot paths by bypassing
//! the V8 slow path and calling directly into native code.

use quote::quote;
use syn::{ItemFn, ReturnType, Type};

use crate::types::get_rc_inner_type;

/// V8 Fast API type mapping
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FastApiType {
    Void,
    Bool,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
}

impl FastApiType {
    /// Get the CTypeInfo for this Fast API type
    pub fn quote_ctype(&self) -> proc_macro2::TokenStream {
        match self {
            FastApiType::Void => quote!(v8::fast_api::Type::Void.as_info()),
            FastApiType::Bool => quote!(v8::fast_api::Type::Bool.as_info()),
            FastApiType::I32 => quote!(v8::fast_api::Type::Int32.as_info()),
            FastApiType::U32 => quote!(v8::fast_api::Type::Uint32.as_info()),
            FastApiType::I64 => quote!(v8::fast_api::Type::Int64.as_info()),
            FastApiType::U64 => quote!(v8::fast_api::Type::Uint64.as_info()),
            FastApiType::F32 => quote!(v8::fast_api::Type::Float32.as_info()),
            FastApiType::F64 => quote!(v8::fast_api::Type::Float64.as_info()),
        }
    }

    /// Get the Rust type for this Fast API type (used in function signature)
    pub fn quote_rust_type(&self) -> proc_macro2::TokenStream {
        match self {
            FastApiType::Void => quote!(()),
            FastApiType::Bool => quote!(bool),
            FastApiType::I32 => quote!(i32),
            FastApiType::U32 => quote!(u32),
            FastApiType::I64 => quote!(i64),
            FastApiType::U64 => quote!(u64),
            FastApiType::F32 => quote!(f32),
            FastApiType::F64 => quote!(f64),
        }
    }
}

/// Check if a type is Fast API compatible and return the mapping
pub fn get_fast_api_type(ty: &Type) -> Option<FastApiType> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let ident = segment.ident.to_string();

            return match ident.as_str() {
                "bool" => Some(FastApiType::Bool),
                "i32" => Some(FastApiType::I32),
                "u32" => Some(FastApiType::U32),
                "i64" => Some(FastApiType::I64),
                "u64" => Some(FastApiType::U64),
                "f32" => Some(FastApiType::F32),
                "f64" => Some(FastApiType::F64),
                _ => None,
            };
        }
    }

    // Handle unit type ()
    if let Type::Tuple(tuple) = ty {
        if tuple.elems.is_empty() {
            return Some(FastApiType::Void);
        }
    }

    None
}

/// Get the Fast API return type from a function's return type
pub fn get_fast_api_return_type(ret: &ReturnType) -> Option<FastApiType> {
    match ret {
        ReturnType::Default => Some(FastApiType::Void),
        ReturnType::Type(_, ty) => get_fast_api_type(ty),
    }
}

/// Generate Fast API code with both slow and fast paths.
///
/// Design:
/// - For functions WITHOUT state: fast path calls original function directly
/// - For functions WITH state: use FastApiCallbackOptions to extract state from data
///
/// State handling (Deno-style):
/// - State is passed as External via FunctionTemplate data
/// - Slow path: extracts from args.data()
/// - Fast path: extracts from options.data
///
/// Important: Fast API with state does NOT use context slots.
/// The state must be passed when creating the FunctionTemplate.
#[allow(clippy::too_many_arguments)]
pub fn generate_fast_api_code(
    input_fn: &ItemFn,
    fn_name: &syn::Ident,
    wrapper_name: &syn::Ident,
    params: &[(syn::Ident, Box<Type>)],
    has_scope: bool,
    has_state: bool,
    state_type: &Option<Type>,
    state_extraction: &proc_macro2::TokenStream,
    arg_extractions: &[proc_macro2::TokenStream],
    call_and_return: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let fast_fn_name = syn::Ident::new(&format!("{}_v8_fast", fn_name), fn_name.span());
    let template_fn_name = syn::Ident::new(&format!("{}_v8_template", fn_name), fn_name.span());
    let cfunction_name = syn::Ident::new(
        &format!("{}_V8_FAST_CALL", fn_name.to_string().to_uppercase()),
        fn_name.span(),
    );
    let cfunction_info_name = syn::Ident::new(
        &format!("{}_V8_FAST_CALL_INFO", fn_name.to_string().to_uppercase()),
        fn_name.span(),
    );

    // Check if all params are Fast API compatible
    let mut fast_param_types: Vec<FastApiType> = Vec::new();
    let mut all_fast_compatible = true;

    for (_, ty) in params {
        if let Some(fast_type) = get_fast_api_type(ty) {
            fast_param_types.push(fast_type);
        } else {
            all_fast_compatible = false;
            break;
        }
    }

    // Check return type
    let fast_return_type = get_fast_api_return_type(&input_fn.sig.output);

    if !all_fast_compatible || fast_return_type.is_none() {
        // Fall back to slow-path only
        return quote! {
            #input_fn

            // Note: fast attribute specified but function has non-primitive types.
            // Falling back to slow path only.

            /// V8 callback wrapper - auto-generated by glue_v8::method
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
    }

    // Fast API is not compatible with functions that use scope
    // (they need scope for V8 operations which isn't available in fast path)
    if has_scope {
        return quote! {
            #input_fn

            // Note: fast attribute specified but function uses scope.
            // Fast API cannot provide scope access. Falling back to slow path only.

            /// V8 callback wrapper - auto-generated by glue_v8::method
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
    }

    // Common: get return type
    let fast_return = fast_return_type.unwrap();

    // Fast API WITH state: use options.data to extract state
    if has_state {
        if let Some(state_ty) = state_type {
            return generate_fast_api_with_state(
                input_fn,
                fn_name,
                wrapper_name,
                &fast_fn_name,
                &template_fn_name,
                &cfunction_name,
                &cfunction_info_name,
                params,
                &fast_param_types,
                fast_return,
                state_ty,
                arg_extractions,
                call_and_return,
            );
        } else {
            // State without type - compilation error
            return quote! {
                #input_fn

                compile_error!("Function has 'state' parameter but no state type specified. Use #[glue_v8::method(fast, state = YourStateType)]");
            };
        }
    }

    // Pure function without scope or state - simplest Fast API case
    generate_fast_api_pure(
        input_fn,
        fn_name,
        wrapper_name,
        &fast_fn_name,
        &template_fn_name,
        &cfunction_name,
        &cfunction_info_name,
        params,
        &fast_param_types,
        fast_return,
        state_extraction,
        arg_extractions,
        call_and_return,
    )
}

/// Generate Fast API for pure functions (no scope, no state)
#[allow(clippy::too_many_arguments)]
fn generate_fast_api_pure(
    input_fn: &ItemFn,
    fn_name: &syn::Ident,
    wrapper_name: &syn::Ident,
    fast_fn_name: &syn::Ident,
    template_fn_name: &syn::Ident,
    cfunction_name: &syn::Ident,
    cfunction_info_name: &syn::Ident,
    params: &[(syn::Ident, Box<Type>)],
    fast_param_types: &[FastApiType],
    fast_return: FastApiType,
    state_extraction: &proc_macro2::TokenStream,
    arg_extractions: &[proc_macro2::TokenStream],
    call_and_return: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    // Generate CTypeInfo array for args
    // Fast API signature: receiver (V8Value) + user args
    let receiver_ctype = quote!(v8::fast_api::Type::V8Value.as_info());
    let arg_ctypes: Vec<_> = fast_param_types.iter().map(|t| t.quote_ctype()).collect();
    let return_ctype = fast_return.quote_ctype();

    // Generate fast function parameters
    let fast_params: Vec<_> = params
        .iter()
        .enumerate()
        .map(|(idx, (name, _))| {
            let rust_type = fast_param_types[idx].quote_rust_type();
            quote!(#name: #rust_type)
        })
        .collect();

    let fast_return_rust = fast_return.quote_rust_type();

    // Arguments to pass to original function
    let call_args_for_fast: Vec<_> = params.iter().map(|(name, _)| quote!(#name)).collect();

    quote! {
        #input_fn

        /// V8 callback wrapper (slow path) - auto-generated by glue_v8::method
        pub fn #wrapper_name(
            scope: &mut v8::PinScope,
            args: v8::FunctionCallbackArguments,
            mut rv: v8::ReturnValue,
        ) {
            #state_extraction
            #(#arg_extractions)*
            #call_and_return
        }

        /// V8 Fast API callback - auto-generated by glue_v8::method(fast)
        ///
        /// This is called directly by V8's optimizing compiler for hot paths.
        /// ~10x faster than the slow path for primitive-only functions.
        extern "C" fn #fast_fn_name(
            _recv: v8::Local<v8::Value>,
            #(#fast_params,)*
            _options: *mut v8::fast_api::FastApiCallbackOptions,
        ) -> #fast_return_rust {
            // Call the original function directly
            #fn_name(#(#call_args_for_fast),*)
        }

        /// CFunctionInfo for the fast call signature
        const #cfunction_info_name: v8::fast_api::CFunctionInfo = v8::fast_api::CFunctionInfo::new(
            #return_ctype,
            &[#receiver_ctype, #(#arg_ctypes),*],
            v8::fast_api::Int64Representation::BigInt,
        );

        /// CFunction definition for V8 Fast API
        pub const #cfunction_name: v8::fast_api::CFunction = v8::fast_api::CFunction::new(
            #fast_fn_name as *const std::ffi::c_void,
            &#cfunction_info_name,
        );

        /// Create a FunctionTemplate with both slow and fast paths
        ///
        /// Use this instead of `v8::Function::new()` for Fast API support.
        ///
        /// # Example
        ///
        /// ```ignore
        /// let template = add_v8_template(scope, None);
        /// let func = template.get_function(scope).unwrap();
        /// ```
        pub fn #template_fn_name<'s>(
            scope: &mut v8::PinScope<'s, '_>,
            data: Option<v8::Local<'s, v8::Value>>,
        ) -> v8::Local<'s, v8::FunctionTemplate> {
            v8::FunctionTemplate::builder(#wrapper_name)
                .data(data.unwrap_or_else(|| v8::undefined(scope).into()))
                .build_fast(scope, &[#cfunction_name])
        }
    }
}

/// Generate Fast API for functions with state (Deno-style approach)
///
/// State is passed via FunctionTemplate data as an External containing a raw pointer.
/// - Slow path: extracts state from args.data()
/// - Fast path: extracts state from options.data
#[allow(clippy::too_many_arguments)]
fn generate_fast_api_with_state(
    input_fn: &ItemFn,
    fn_name: &syn::Ident,
    wrapper_name: &syn::Ident,
    fast_fn_name: &syn::Ident,
    template_fn_name: &syn::Ident,
    cfunction_name: &syn::Ident,
    cfunction_info_name: &syn::Ident,
    params: &[(syn::Ident, Box<Type>)],
    fast_param_types: &[FastApiType],
    fast_return: FastApiType,
    state_type: &Type,
    arg_extractions: &[proc_macro2::TokenStream],
    call_and_return: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    // Generate CTypeInfo array for args
    // Fast API signature: receiver (V8Value) + user args + CallbackOptions
    let receiver_ctype = quote!(v8::fast_api::Type::V8Value.as_info());
    let arg_ctypes: Vec<_> = fast_param_types.iter().map(|t| t.quote_ctype()).collect();
    let options_ctype = quote!(v8::fast_api::Type::CallbackOptions.as_info());
    let return_ctype = fast_return.quote_ctype();

    // Generate fast function parameters
    let fast_params: Vec<_> = params
        .iter()
        .enumerate()
        .map(|(idx, (name, _))| {
            let rust_type = fast_param_types[idx].quote_rust_type();
            quote!(#name: #rust_type)
        })
        .collect();

    let fast_return_rust = fast_return.quote_rust_type();

    // Arguments to pass to original function (with state)
    let call_args_for_fast: Vec<_> = params.iter().map(|(name, _)| quote!(#name)).collect();

    // Determine the inner type for state (unwrap Rc if present)
    let inner_state_type = if let Some(inner) = get_rc_inner_type(state_type) {
        inner.clone()
    } else {
        state_type.clone()
    };

    let state_ty_str = quote!(#state_type).to_string();

    quote! {
        #input_fn

        /// V8 callback wrapper (slow path) - auto-generated by glue_v8::method(fast, state)
        ///
        /// State is extracted from function data (External), NOT context slots.
        pub fn #wrapper_name(
            scope: &mut v8::PinScope,
            args: v8::FunctionCallbackArguments,
            mut rv: v8::ReturnValue,
        ) {
            // Extract state from function data (External containing Rc<State>)
            let state: #state_type = unsafe {
                let data = args.data();

                if data.is_undefined() || data.is_null() {
                    let msg = v8::String::new(scope, concat!("internal error: state data not set for ", #state_ty_str)).unwrap();
                    let err = v8::Exception::error(scope, msg);
                    scope.throw_exception(err);
                    return;
                }

                let external = v8::Local::<v8::External>::try_from(data).unwrap();
                let ptr = external.value() as *const #inner_state_type;
                std::rc::Rc::clone(&*std::mem::ManuallyDrop::new(std::rc::Rc::from_raw(ptr)))
            };

            #(#arg_extractions)*
            #call_and_return
        }

        /// V8 Fast API callback - auto-generated by glue_v8::method(fast, state)
        ///
        /// This is called directly by V8's optimizing compiler for hot paths.
        /// State is extracted from FastApiCallbackOptions.data.
        extern "C" fn #fast_fn_name<'s>(
            _recv: v8::Local<v8::Value>,
            #(#fast_params,)*
            options: *mut v8::fast_api::FastApiCallbackOptions<'s>,
        ) -> #fast_return_rust {
            // SAFETY: options is valid during fast call, data was set to External
            let state: &#inner_state_type = unsafe {
                let options = &*options;
                let external = v8::Local::<v8::External>::cast_unchecked(options.data);
                &*(external.value() as *const #inner_state_type)
            };

            // Wrap in Rc for the function call (without incrementing refcount)
            let state: std::mem::ManuallyDrop<std::rc::Rc<#inner_state_type>> = unsafe {
                std::mem::ManuallyDrop::new(std::rc::Rc::from_raw(state))
            };

            #fn_name(&state, #(#call_args_for_fast),*)
        }

        /// CFunctionInfo for the fast call signature (with CallbackOptions for state)
        const #cfunction_info_name: v8::fast_api::CFunctionInfo = v8::fast_api::CFunctionInfo::new(
            #return_ctype,
            &[#receiver_ctype, #(#arg_ctypes,)* #options_ctype],
            v8::fast_api::Int64Representation::BigInt,
        );

        /// CFunction definition for V8 Fast API
        pub const #cfunction_name: v8::fast_api::CFunction = v8::fast_api::CFunction::new(
            #fast_fn_name as *const std::ffi::c_void,
            &#cfunction_info_name,
        );

        /// Create a FunctionTemplate with both slow and fast paths
        ///
        /// IMPORTANT: State is passed via External, NOT context slots.
        /// The Rc is NOT cloned - the caller must ensure the state outlives the function.
        ///
        /// # Example
        ///
        /// ```ignore
        /// let state = Rc::new(MyState { ... });
        /// let template = my_fn_v8_template(scope, &state);
        /// let func = template.get_function(scope).unwrap();
        /// ```
        pub fn #template_fn_name<'s>(
            scope: &mut v8::PinScope<'s, '_>,
            state: &std::rc::Rc<#inner_state_type>,
        ) -> v8::Local<'s, v8::FunctionTemplate> {
            // Create External containing raw pointer to inner state
            // SAFETY: The Rc ensures the state lives long enough, and we use ManuallyDrop
            // in both paths to avoid double-free
            let ptr = std::rc::Rc::as_ptr(state);
            let external = v8::External::new(scope, ptr as *mut std::ffi::c_void);

            v8::FunctionTemplate::builder(#wrapper_name)
                .data(external.into())
                .build_fast(scope, &[#cfunction_name])
        }
    }
}

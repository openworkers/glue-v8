//! V8 Glue - Rust to V8 binding macros for OpenWorkers
//!
//! Generate V8 callback boilerplate from Rust functions.
//!
//! ## Fast API Support
//!
//! The `fast` attribute enables V8 Fast API for functions with primitive arguments.
//! Fast API can be ~10x faster for hot functions.
//!
//! ### Pure functions (no scope, no state)
//!
//! ```ignore
//! #[glue_v8::method(fast)]
//! fn add(a: i32, b: i32) -> i32 {
//!     a + b
//! }
//!
//! // Registration:
//! let template = add_v8_template(scope, None);
//! let func = template.get_function(scope).unwrap();
//! ```
//!
//! ### Functions with state (Deno-style)
//!
//! ```ignore
//! #[glue_v8::method(fast, state = Rc<TimerState>)]
//! fn schedule_timeout(state: &Rc<TimerState>, id: u64, delay: u64) {
//!     let _ = state.scheduler_tx.send(ScheduleTimeout(id, delay));
//! }
//!
//! // Registration (state is passed via External, not context slot):
//! let state = Rc::new(TimerState { ... });
//! let template = schedule_timeout_v8_template(scope, &state);
//! let func = template.get_function(scope).unwrap();
//! ```
//!
//! Requirements for Fast API:
//! - Only primitive types: bool, i32, u32, i64, u64, f32, f64
//! - No scope parameter (cannot use V8 APIs in fast path)
//! - Return type must be a primitive or void
//! - State must be Rc<T> (passed via FunctionTemplate data, not context slot)

mod codegen;
mod fast;
mod parse;
mod types;

use proc_macro::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, Pat, ReturnType, parse_macro_input};

use codegen::{generate_arg_extractions, generate_call_and_return, generate_state_extraction};
use fast::generate_fast_api_code;
use parse::MethodAttrs;
use types::is_result_type;

/// Generate a V8 callback wrapper for a Rust function.
///
/// # Examples
///
/// Basic function (no state):
/// ```ignore
/// #[glue_v8::method]
/// fn add(scope: &mut v8::PinScope, a: f64, b: f64) -> f64 {
///     a + b
/// }
/// ```
///
/// With state from context slot:
/// ```ignore
/// #[glue_v8::method(state = TimerState)]
/// fn schedule_timeout(scope: &mut v8::PinScope, state: &TimerState, id: u64, delay: u64) {
///     let _ = state.scheduler_tx.send(SchedulerMessage::ScheduleTimeout(id, delay));
/// }
/// ```
///
/// With custom JS name:
/// ```ignore
/// #[glue_v8::method(name = "setTimeout")]
/// fn set_timeout(scope: &mut v8::PinScope, delay: f64) -> u64 { ... }
/// ```
///
/// With Result return type (throws on Err):
/// ```ignore
/// #[glue_v8::method]
/// fn parse_json(scope: &mut v8::PinScope, input: String) -> Result<serde_json::Value, String> {
///     serde_json::from_str(&input).map_err(|e| e.to_string())
/// }
/// ```
///
/// With Promise (returns JS Promise):
/// ```ignore
/// #[glue_v8::method(promise)]
/// fn fetch_data(scope: &mut v8::PinScope, url: String) -> Result<String, String> {
///     // Ok(value) → Promise.resolve(value)
///     // Err(msg)  → Promise.reject(new Error(msg))
///     Ok("data".to_string())
/// }
/// ```
///
/// With optional parameters:
/// ```ignore
/// #[glue_v8::method]
/// fn greet(scope: &mut v8::PinScope, name: String, title: Option<String>) -> String {
///     // greet("Alice") → title is None
///     // greet("Alice", "Dr.") → title is Some("Dr.")
///     match title {
///         Some(t) => format!("{} {}", t, name),
///         None => name,
///     }
/// }
/// ```
///
/// With Fast API (for hot paths with primitive types):
/// ```ignore
/// #[glue_v8::method(fast)]
/// fn add(a: i32, b: i32) -> i32 {
///     a + b
/// }
///
/// // With state - state is passed via FunctionTemplate data
/// #[glue_v8::method(fast, state = Rc<TimerState>)]
/// fn schedule_timeout(state: &Rc<TimerState>, id: u64, delay: u64) {
///     let _ = state.scheduler_tx.send(SchedulerMessage::ScheduleTimeout(id, delay));
/// }
/// ```
///
/// Note: Fast API functions generate both slow and fast paths.
/// Use `{fn_name}_v8_template(scope, state_external)` to register with FunctionTemplate.
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
    let arg_extractions = generate_arg_extractions(&params);

    // Generate state extraction if needed
    let state_extraction = generate_state_extraction(has_state, &attrs.state_type);

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

    let call_and_return = generate_call_and_return(
        fn_name,
        &call_args,
        has_return,
        returns_result,
        attrs.promise,
    );

    // Generate the expanded code
    let expanded = if attrs.fast {
        // Fast API mode: generate both slow and fast paths
        generate_fast_api_code(
            &input_fn,
            fn_name,
            &wrapper_name,
            &params,
            has_scope,
            has_state,
            &attrs.state_type,
            &state_extraction,
            &arg_extractions,
            &call_and_return,
        )
    } else {
        // Standard mode: only slow path
        quote! {
            #input_fn

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
        }
    };

    TokenStream::from(expanded)
}

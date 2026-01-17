//! Integration tests for gv8 macro
//!
//! These tests verify that the macro generates correct V8 callback wrappers.

use std::pin::pin;
use std::rc::Rc;
use std::sync::Once;

static INIT: Once = Once::new();

fn init_v8() {
    INIT.call_once(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
    });
}

// ============================================================================
// Test: Basic function with simple args
// ============================================================================

#[gv8::method]
fn add(_scope: &mut v8::PinScope, a: f64, b: f64) -> f64 {
    a + b
}

#[test]
fn test_basic_add() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, add_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "add").unwrap();
    global.set(scope, key.into(), func.into());

    // Execute: add(2, 3)
    let code = v8::String::new(scope, "add(2, 3)").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert!(result.is_number());
    assert_eq!(result.number_value(scope).unwrap(), 5.0);
}

// ============================================================================
// Test: Function with string args
// ============================================================================

#[gv8::method]
fn concat(_scope: &mut v8::PinScope, a: String, b: String) -> String {
    format!("{}{}", a, b)
}

#[test]
fn test_string_concat() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, concat_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "concat").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "concat('Hello, ', 'World!')").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert!(result.is_string());
    assert_eq!(result.to_rust_string_lossy(scope), "Hello, World!");
}

// ============================================================================
// Test: Function with no return value
// ============================================================================

thread_local! {
    static SIDE_EFFECT: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
}

#[gv8::method]
fn set_value(_scope: &mut v8::PinScope, val: i32) {
    SIDE_EFFECT.with(|v| v.set(val));
}

#[test]
fn test_no_return_value() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    SIDE_EFFECT.with(|v| v.set(0));

    let func = v8::Function::new(scope, set_value_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "setValue").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "setValue(42)").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert!(result.is_undefined());
    SIDE_EFFECT.with(|v| assert_eq!(v.get(), 42));
}

// ============================================================================
// Test: Function with Option<T> parameter
// ============================================================================

#[gv8::method]
fn greet(_scope: &mut v8::PinScope, name: String, title: Option<String>) -> String {
    match title {
        Some(t) => format!("{} {}", t, name),
        None => name,
    }
}

#[test]
fn test_option_with_value() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, greet_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "greet").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "greet('Alice', 'Dr.')").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert_eq!(result.to_rust_string_lossy(scope), "Dr. Alice");
}

#[test]
fn test_option_undefined() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, greet_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "greet").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "greet('Bob', undefined)").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert_eq!(result.to_rust_string_lossy(scope), "Bob");
}

#[test]
fn test_option_null() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, greet_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "greet").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "greet('Charlie', null)").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert_eq!(result.to_rust_string_lossy(scope), "Charlie");
}

#[test]
fn test_option_missing_arg() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, greet_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "greet").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "greet('Dave')").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert_eq!(result.to_rust_string_lossy(scope), "Dave");
}

// ============================================================================
// Test: Function with Result return type
// ============================================================================

#[gv8::method]
fn parse_number(_scope: &mut v8::PinScope, input: String) -> Result<f64, String> {
    input.parse::<f64>().map_err(|e| e.to_string())
}

#[test]
fn test_result_ok() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, parse_number_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "parseNumber").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "parseNumber('42.5')").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert!(result.is_number());
    assert_eq!(result.number_value(scope).unwrap(), 42.5);
}

#[test]
fn test_result_err_throws() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);
    let tc = pin!(v8::TryCatch::new(scope));
    let mut tc = tc.init();

    let func = v8::Function::new(&mut tc, parse_number_v8).unwrap();
    let global = tc.get_current_context().global(&tc);
    let key = v8::String::new(&mut tc, "parseNumber").unwrap();
    global.set(&mut tc, key.into(), func.into());

    let code = v8::String::new(&mut tc, "parseNumber('not a number')").unwrap();
    let script = v8::Script::compile(&mut tc, code, None).unwrap();
    let result = script.run(&mut tc);

    assert!(result.is_none());
    assert!(tc.has_caught());
}

// ============================================================================
// Test: Function with V8 callback (Function type)
// ============================================================================

#[gv8::method]
fn call_twice(scope: &mut v8::PinScope, callback: v8::Local<v8::Function>, value: f64) -> f64 {
    let recv = v8::undefined(scope).into();
    let arg = v8::Number::new(scope, value).into();

    // Call once
    let result1 = callback.call(scope, recv, &[arg]).unwrap();
    let val1 = result1.number_value(scope).unwrap_or(0.0);

    // Call twice with result
    let arg2 = v8::Number::new(scope, val1).into();
    let result2 = callback.call(scope, recv, &[arg2]).unwrap();
    result2.number_value(scope).unwrap_or(0.0)
}

#[test]
fn test_callback_function() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, call_twice_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "callTwice").unwrap();
    global.set(scope, key.into(), func.into());

    // callTwice(x => x * 2, 5) = (5 * 2) * 2 = 20
    let code = v8::String::new(scope, "callTwice(x => x * 2, 5)").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert!(result.is_number());
    assert_eq!(result.number_value(scope).unwrap(), 20.0);
}

#[test]
fn test_callback_type_error() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);
    let tc = pin!(v8::TryCatch::new(scope));
    let mut tc = tc.init();

    let func = v8::Function::new(&mut tc, call_twice_v8).unwrap();
    let global = tc.get_current_context().global(&tc);
    let key = v8::String::new(&mut tc, "callTwice").unwrap();
    global.set(&mut tc, key.into(), func.into());

    // Pass a string instead of function
    let code = v8::String::new(&mut tc, "callTwice('not a function', 5)").unwrap();
    let script = v8::Script::compile(&mut tc, code, None).unwrap();
    let result = script.run(&mut tc);

    assert!(result.is_none());
    assert!(tc.has_caught());

    let exception = tc.exception().unwrap();
    let msg = exception.to_rust_string_lossy(&tc);
    assert!(msg.contains("must be a Function"));
}

// ============================================================================
// Test: Function with state from context slot
// ============================================================================

struct Counter {
    value: std::cell::Cell<i32>,
}

#[gv8::method(state = Rc<Counter>)]
fn increment(_scope: &mut v8::PinScope, state: &Rc<Counter>, amount: i32) -> i32 {
    let new_val = state.value.get() + amount;
    state.value.set(new_val);
    new_val
}

#[test]
fn test_state_from_slot() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    // Store state in context slot (matching runtime pattern: store raw, wrap in Rc)
    let counter = Counter {
        value: std::cell::Cell::new(10),
    };
    scope.get_current_context().set_slot(Rc::new(counter));

    let func = v8::Function::new(scope, increment_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "increment").unwrap();
    global.set(scope, key.into(), func.into());

    // First call: 10 + 5 = 15
    let code = v8::String::new(scope, "increment(5)").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();
    assert_eq!(result.number_value(scope).unwrap(), 15.0);

    // Second call: 15 + 7 = 22
    let code = v8::String::new(scope, "increment(7)").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();
    assert_eq!(result.number_value(scope).unwrap(), 22.0);
}

// ============================================================================
// Test: Promise with Result
// ============================================================================

#[gv8::method(promise)]
fn async_divide(_scope: &mut v8::PinScope, a: f64, b: f64) -> Result<f64, String> {
    if b == 0.0 {
        Err("Division by zero".to_string())
    } else {
        Ok(a / b)
    }
}

#[test]
fn test_promise_resolve() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, async_divide_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "asyncDivide").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "asyncDivide(10, 2)").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert!(result.is_promise());
    let promise: v8::Local<v8::Promise> = result.try_into().unwrap();
    assert_eq!(promise.state(), v8::PromiseState::Fulfilled);
    assert_eq!(promise.result(scope).number_value(scope).unwrap(), 5.0);
}

#[test]
fn test_promise_reject() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, async_divide_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "asyncDivide").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "asyncDivide(10, 0)").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert!(result.is_promise());
    let promise: v8::Local<v8::Promise> = result.try_into().unwrap();
    assert_eq!(promise.state(), v8::PromiseState::Rejected);
}

// ============================================================================
// Test: Wrong argument type throws TypeError
// ============================================================================

#[test]
fn test_wrong_arg_type() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);
    let tc = pin!(v8::TryCatch::new(scope));
    let mut tc = tc.init();

    let func = v8::Function::new(&mut tc, add_v8).unwrap();
    let global = tc.get_current_context().global(&tc);
    let key = v8::String::new(&mut tc, "add").unwrap();
    global.set(&mut tc, key.into(), func.into());

    // Pass strings instead of numbers
    let code = v8::String::new(&mut tc, "add('not', 'numbers')").unwrap();
    let script = v8::Script::compile(&mut tc, code, None).unwrap();
    let result = script.run(&mut tc);

    assert!(result.is_none());
    assert!(tc.has_caught());
}

// ============================================================================
// Test: Uint8Array parameter
// ============================================================================

#[gv8::method]
fn sum_bytes(_scope: &mut v8::PinScope, data: v8::Local<v8::Uint8Array>) -> u32 {
    let len = data.byte_length();
    let mut bytes = vec![0u8; len];
    data.copy_contents(&mut bytes);
    bytes.iter().map(|&b| b as u32).sum()
}

#[test]
fn test_uint8array() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, sum_bytes_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "sumBytes").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "sumBytes(new Uint8Array([1, 2, 3, 4, 5]))").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert!(result.is_number());
    assert_eq!(result.number_value(scope).unwrap(), 15.0); // 1+2+3+4+5
}

#[test]
fn test_uint8array_type_error() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);
    let tc = pin!(v8::TryCatch::new(scope));
    let mut tc = tc.init();

    let func = v8::Function::new(&mut tc, sum_bytes_v8).unwrap();
    let global = tc.get_current_context().global(&tc);
    let key = v8::String::new(&mut tc, "sumBytes").unwrap();
    global.set(&mut tc, key.into(), func.into());

    // Pass a regular array instead of Uint8Array
    let code = v8::String::new(&mut tc, "sumBytes([1, 2, 3])").unwrap();
    let script = v8::Script::compile(&mut tc, code, None).unwrap();
    let result = script.run(&mut tc);

    assert!(result.is_none());
    assert!(tc.has_caught());

    let exception = tc.exception().unwrap();
    let msg = exception.to_rust_string_lossy(&tc);
    assert!(msg.contains("must be a Uint8Array"));
}

// ============================================================================
// Test: Multiple args with different types
// ============================================================================

#[gv8::method]
fn format_message(
    _scope: &mut v8::PinScope,
    prefix: String,
    count: i32,
    suffix: Option<String>,
) -> String {
    let suffix = suffix.unwrap_or_else(|| "items".to_string());
    format!("{}: {} {}", prefix, count, suffix)
}

#[test]
fn test_mixed_args() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, format_message_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "formatMessage").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "formatMessage('Total', 42, 'things')").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert_eq!(result.to_rust_string_lossy(scope), "Total: 42 things");
}

#[test]
fn test_mixed_args_optional_missing() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, format_message_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "formatMessage").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "formatMessage('Count', 7)").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();

    assert_eq!(result.to_rust_string_lossy(scope), "Count: 7 items");
}

// ============================================================================
// Test: Boolean return value
// ============================================================================

#[gv8::method]
fn is_even(_scope: &mut v8::PinScope, n: i32) -> bool {
    n % 2 == 0
}

#[test]
fn test_bool_return() {
    init_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);

    let func = v8::Function::new(scope, is_even_v8).unwrap();
    let global = scope.get_current_context().global(scope);
    let key = v8::String::new(scope, "isEven").unwrap();
    global.set(scope, key.into(), func.into());

    let code = v8::String::new(scope, "isEven(4)").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();
    assert!(result.is_true());

    let code = v8::String::new(scope, "isEven(5)").unwrap();
    let script = v8::Script::compile(scope, code, None).unwrap();
    let result = script.run(scope).unwrap();
    assert!(result.is_false());
}

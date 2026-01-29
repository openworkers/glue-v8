# glue_v8 - V8 Glue for OpenWorkers

Proc-macro crate that generates V8 callback boilerplate from Rust functions.

## Usage

```rust
use std::rc::Rc;

// Basic function
#[glue_v8::method]
fn add(_scope: &mut v8::PinScope, a: f64, b: f64) -> f64 {
    a + b
}

// With state from context slot
#[glue_v8::method(state = Rc<MyState>)]
fn get_count(_scope: &mut v8::PinScope, state: &Rc<MyState>) -> i32 {
    state.count.get()
}

// With Result (Err throws JS exception)
#[glue_v8::method]
fn parse(_scope: &mut v8::PinScope, input: String) -> Result<f64, String> {
    input.parse().map_err(|e| format!("{}", e))
}

// With Promise (returns JS Promise)
#[glue_v8::method(promise)]
fn async_op(_scope: &mut v8::PinScope, val: i32) -> Result<i32, String> {
    if val > 0 { Ok(val * 2) } else { Err("must be positive".into()) }
}

// Optional parameters
#[glue_v8::method]
fn greet(_scope: &mut v8::PinScope, name: String, title: Option<String>) -> String {
    match title {
        Some(t) => format!("{} {}", t, name),
        None => name,
    }
}
```

## Generated Code

The macro generates a `{fn_name}_v8` wrapper function that:
- Extracts arguments from `FunctionCallbackArguments`
- Converts types using `serde_v8`
- Handles V8 Local types directly (Function, Uint8Array, etc.)
- Retrieves state from context slots
- Converts return values back to V8
- Throws exceptions on errors

## Supported Types

**Parameters:**
- Primitives: `i32`, `u32`, `f64`, `bool`, `String`
- Optional: `Option<T>` (None for undefined/null/missing)
- V8 Local types: `v8::Local<v8::Function>`, `v8::Local<v8::Uint8Array>`, etc.
- Any type implementing `serde::Deserialize`

**Return types:**
- Primitives and `String`
- `Result<T, E>` (Err throws exception)
- Any type implementing `serde::Serialize`

**Attributes:**
- `state = Rc<T>` - Extract state from context slot
- `promise` - Return a JS Promise
- `name = "jsName"` - Custom JS function name

## Running Tests

```bash
cargo test
```

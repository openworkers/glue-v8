//! Attribute parsing for glue_v8 macros.

use proc_macro::TokenStream;
use syn::Type;

/// Parsed attributes for #[glue_v8::method]
pub struct MethodAttrs {
    pub js_name: Option<String>,
    pub state_type: Option<Type>,
    pub promise: bool,
    pub fast: bool,
}

impl MethodAttrs {
    pub fn parse(attr: TokenStream) -> Self {
        use std::cell::RefCell;

        let js_name: RefCell<Option<String>> = RefCell::new(None);
        let state_type: RefCell<Option<Type>> = RefCell::new(None);
        let promise: RefCell<bool> = RefCell::new(false);
        let fast: RefCell<bool> = RefCell::new(false);

        if !attr.is_empty() {
            let parser = syn::meta::parser(|meta| {
                if meta.path.is_ident("state") {
                    // Parse as type (handles generics like Rc<T>)
                    let value: Type = meta.value()?.parse()?;
                    *state_type.borrow_mut() = Some(value);
                    Ok(())
                } else if meta.path.is_ident("name") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    *js_name.borrow_mut() = Some(value.value());
                    Ok(())
                } else if meta.path.is_ident("promise") {
                    *promise.borrow_mut() = true;
                    Ok(())
                } else if meta.path.is_ident("fast") {
                    *fast.borrow_mut() = true;
                    Ok(())
                } else {
                    Err(meta.error(
                        "expected `state = Type`, `name = \"jsName\"`, `promise`, or `fast`",
                    ))
                }
            });

            // Try parsing as meta items
            if syn::parse::Parser::parse(parser, attr.clone()).is_err() {
                // Fall back to bare string literal
                if let Ok(lit) = syn::parse::<syn::LitStr>(attr) {
                    *js_name.borrow_mut() = Some(lit.value());
                }
            }
        }

        Self {
            js_name: js_name.into_inner(),
            state_type: state_type.into_inner(),
            promise: promise.into_inner(),
            fast: fast.into_inner(),
        }
    }
}

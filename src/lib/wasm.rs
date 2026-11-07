use std::cell::RefCell;

use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

thread_local! {
    static LOG_CALLBACK: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
pub fn set_log_callback(callback: js_sys::Function) {
    LOG_CALLBACK.with(|current| *current.borrow_mut() = Some(callback));
}

pub(crate) fn emit_log(message: String) {
    LOG_CALLBACK.with(|callback| {
        if let Some(callback) = callback.borrow().as_ref() {
            if let Err(error) = callback.call1(&JsValue::NULL, &JsValue::from_str(&message)) {
                web_sys::console::error_1(&error);
            }
        }
    });
}

#[wasm_bindgen]
pub fn run_example(name: &str) -> Result<(), JsValue> {
    match name {
        "byz_cascading_quorum" => crate::run_byz_cascading_quorum(),
        "byz_cascading_quorum_v2" => crate::run_byz_cascading_quorum_v2(),
        "byz_time" => crate::run_byz_time(),
        "utc_consensus" => crate::run_utc_consensus(),
        "p2p_node" => {
            return Err(JsValue::from_str(
                "p2p_node runs through the browser networking client, not the native Rust binary.",
            ));
        }
        _ => {
            return Err(JsValue::from_str(&format!(
                "Unknown binary example: {}",
                name
            )));
        }
    }

    Ok(())
}

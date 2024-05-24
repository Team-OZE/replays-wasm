mod utils;
mod replay;

use serde::Serialize;
use wasm_bindgen::prelude::*;
use crate::replay::Replay;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
pub fn parse_replay_file(bytes: &[u8]) -> String {
    let replay = Replay::from_bytes(&bytes);
    return serde_json::to_string(&replay).unwrap();
}

#[wasm_bindgen]
pub fn debug_init() {
    utils::set_panic_hook();
    console_log::init().unwrap_or_default();
}
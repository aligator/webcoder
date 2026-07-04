pub mod core;

#[cfg(target_arch = "wasm32")]
pub mod app;

#[cfg(not(target_arch = "wasm32"))]
pub mod server;

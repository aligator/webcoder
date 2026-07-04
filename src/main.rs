#[cfg(target_arch = "wasm32")]
fn main() {
    yew::Renderer::<webcoder::app::App>::new().render();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    println!("Webcoder WASM is a browser app. Run it with `trunk serve`.");
}

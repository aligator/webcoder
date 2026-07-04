#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
    if let Err(error) = webcoder::server::run().await {
        eprintln!("server error: {error}");
        std::process::exit(1);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}

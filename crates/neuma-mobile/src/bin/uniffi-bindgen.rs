//! `cargo run -p neuma-mobile --features bindgen --bin uniffi-bindgen -- generate --library
//! target/debug/libneuma_mobile.so --language kotlin --out-dir out`
fn main() {
    uniffi::uniffi_bindgen_main()
}

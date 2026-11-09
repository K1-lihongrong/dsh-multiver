//! A2-8: cargo run --bin generate_bindings -- ../src/api/bindings.ts
fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "../src/api/bindings.ts".to_string());
    if let Err(e) = dsh_multiver_lib::generate_bindings(&path) {
        eprintln!("生成 bindings 失败: {e}");
        std::process::exit(1);
    }
    println!("bindings 已生成: {path}");
}

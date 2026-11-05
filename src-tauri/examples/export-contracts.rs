fn main() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/app/contracts.ts");
    std::fs::write(path, mediacenter_lib::contracts::typescript()).expect("write generated types");
}

fn main() {
    slint_build::compile("ui/speed-gauge.slint").unwrap();
    println!("cargo:rerun-if-changed=ui/speed-gauge.slint");

    // demo.slint 用 ComponentContainer + component-factory（Slint experimental），
    // 编译器只在 SLINT_ENABLE_EXPERIMENTAL_FEATURES 存在时注册这些元素。
    std::env::set_var("SLINT_ENABLE_EXPERIMENTAL_FEATURES", "1");
    slint_build::compile("ui/demo.slint").unwrap();
    std::env::remove_var("SLINT_ENABLE_EXPERIMENTAL_FEATURES");
    println!("cargo:rerun-if-changed=ui/demo.slint");
}

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=../assets/icon.ico");
    winresource::WindowsResource::new()
        .set_icon("../assets/icon.ico")
        .set("ProductName", "Better Sonar")
        .set("FileDescription", "Better Sonar")
        .compile()
}

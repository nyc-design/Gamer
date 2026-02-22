fn main() {
    println!("cargo:rustc-link-lib=Xcomposite");
    println!("cargo:rustc-link-lib=Xdamage");
    println!("cargo:rustc-link-lib=Xfixes");
}

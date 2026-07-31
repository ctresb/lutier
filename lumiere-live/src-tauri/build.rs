fn main() {
    // o screencapturekit linka codigo swift; o runtime (@rpath/
    // libswift_Concurrency.dylib etc) mora em /usr/lib/swift no
    // macos. rustc-link-arg de build.rs de DEPENDENCIA nao propaga
    // pro binario final, entao o rpath tem que sair daqui - sem ele
    // o app crasha no launch com "Library missing".
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    tauri_build::build()
}

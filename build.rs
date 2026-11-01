fn main() {
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    tonic_prost_build::configure()
        .file_descriptor_set_path(output.join("canopy-remote-v1.bin"))
        .compile_protos(
            &[
                "proto/canopy/remote/v1/canopy.proto",
                "proto/grpc/health/v1/health.proto",
            ],
            &["proto"],
        )
        .expect("compile the native Canopy gRPC contract (protoc is required)");
    println!("cargo:rerun-if-changed=proto/canopy/remote/v1/canopy.proto");
    println!("cargo:rerun-if-changed=proto/grpc/health/v1/health.proto");
    tonic_prost_build::configure()
        .file_descriptor_set_path(output.join("canopy-api-v1.bin"))
        .compile_protos(&["proto/canopy/api/v1/api.proto"], &["proto"])
        .expect("compile the typed Canopy API contract");
    println!("cargo:rerun-if-changed=proto/canopy/api/v1/api.proto");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("native/video_preview.m")
            .file("native/attachment_preview.m")
            .file("native/html_page.m")
            .flag("-fobjc-arc")
            .compile("canopy_video");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=CoreMedia");
        println!("cargo:rustc-link-lib=framework=QuartzCore");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=Quartz");
        println!("cargo:rustc-link-lib=framework=WebKit");
        println!("cargo:rerun-if-changed=native/html_page.m");
        println!("cargo:rerun-if-changed=native/attachment_preview.m");
        println!("cargo:rerun-if-changed=native/video_preview.m");
    }
}

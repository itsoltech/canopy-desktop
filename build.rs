fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("native/video_preview.m")
            .file("native/attachment_preview.m")
            .flag("-fobjc-arc")
            .compile("canopy_video");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=CoreMedia");
        println!("cargo:rustc-link-lib=framework=QuartzCore");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=Quartz");
        println!("cargo:rerun-if-changed=native/attachment_preview.m");
        println!("cargo:rerun-if-changed=native/video_preview.m");
    }
}

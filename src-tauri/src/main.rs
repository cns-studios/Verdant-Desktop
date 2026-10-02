#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")] {
        // Blank screen workaround for Linux + NVIDIA
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        println!("Use Blank screen workardoun for Linux \"WEBKIT_DISABLE_DMABUF_RENDERER=1\"");
    }

    app_lib::run();
}

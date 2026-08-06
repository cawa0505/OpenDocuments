use std::fs;
use std::path::Path;

fn main() {
    // Re-run build.rs only if dist directory changes or is created.
    println!("cargo:rerun-if-changed=../../apps/webui/dist");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dist_dir = Path::new(&manifest_dir).join("../../apps/webui/dist");
    
    if !dist_dir.exists() {
        fs::create_dir_all(&dist_dir).unwrap_or_else(|e| {
            println!("cargo:warning=Failed to create dist directory: {}", e);
        });
    }
    
    let index_html = dist_dir.join("index.html");
    if !index_html.exists() {
        let dummy_html = "<!DOCTYPE html>\
<html>\
<head>\
    <meta charset=\"UTF-8\">\
    <title>OpenDocuments</title>\
    <style>\
        body { font-family: sans-serif; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; background: #f9fafb; color: #111827; }\
        .container { text-align: center; padding: 2rem; border-radius: 0.5rem; background: white; box-shadow: 0 1px 3px rgba(0,0,0,0.1); max-width: 28rem; }\
        h1 { font-size: 1.5rem; margin-bottom: 1rem; }\
        p { color: #4b5563; font-size: 0.875rem; line-height: 1.5; }\
    </style>\
</head>\
<body>\
    <div class=\"container\">\
        <h1>WebUI Assets Not Compiled</h1>\
        <p>This binary was compiled without built frontend assets. If you are developing locally, run <code>make install</code> or <code>npm run build</code> inside the <code>apps/webui</code> directory to build the production assets.</p>\
    </div>\
</body>\
</html>";
        fs::write(&index_html, dummy_html).unwrap_or_else(|e| {
            println!("cargo:warning=Failed to write dummy index.html: {}", e);
        });
    }
}

use std::path::Path;
use std::env;
use std::process;
use opendoc_parser::parse_file;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("=== OpenDocuments Rust CLI Parser Agent ===");
        eprintln!("Usage: opendoc-srv <file_path> <workspace_id> <collection_id>");
        process::exit(1);
    }

    let file_path = Path::new(&args[1]);
    let workspace_id = &args[2];
    let collection_id = &args[3];

    match parse_file(file_path, workspace_id, collection_id).await {
        Ok(chunks) => {
            // 序列化為乾淨的 JSON 吐向 STDOUT
            match serde_json::to_string_pretty(&chunks) {
                Ok(json_str) => {
                    println!("{}", json_str);
                }
                Err(e) => {
                    eprintln!("Serialization error: {}", e);
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Parsing failed: {}", e);
            process::exit(1);
        }
    }
}

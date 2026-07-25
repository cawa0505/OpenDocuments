use std::path::Path;
use std::env;
use opendoc_parser::parse_file;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: opendoc-srv <file_path> <workspace_id> <collection_id> [original_name]");
        std::process::exit(1);
    }

    let file_path_str = &args[1];
    let workspace_id = &args[2];
    let collection_id = &args[3];
    
    // 💡 獲取可選的 original_name，完美應對上傳隨機 hash 暫存檔
    let original_name = if args.len() >= 5 {
        Some(args[4].as_str())
    } else {
        None
    };

    let file_path = Path::new(file_path_str);
    if !file_path.exists() {
        eprintln!("Error: File not found at {:?}", file_path);
        std::process::exit(1);
    }

    match parse_file(file_path, original_name, workspace_id, collection_id).await {
        Ok(chunks) => {
            match serde_json::to_string_pretty(&chunks) {
                Ok(json_str) => {
                    println!("{}", json_str);
                }
                Err(e) => {
                    eprintln!("Error: Failed to serialize chunks to JSON: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error: Ingestion failed: {}", e);
            std::process::exit(1);
        }
    }
}

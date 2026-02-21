//! Generate JSON Schema for Synbot config.json.
//!
//! Run with:
//!   cargo run --example generate_config_schema --features schema
//!
//! Save to file:
//!   cargo run --example generate_config_schema --features schema -- --output config.schema.json
//!   cargo run --example generate_config_schema --features schema -- -o docs/config.schema.json

use std::env;
use std::io::{self, Write};
use std::path::Path;

fn main() {
    let schema = synbot::config::config_json_schema();
    let json = serde_json::to_string_pretty(&schema).expect("serialize schema");

    let args: Vec<String> = env::args().collect();
    let mut output_path: Option<&str> = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--output" || args[i] == "-o" {
            i += 1;
            if i < args.len() {
                output_path = Some(&args[i]);
            }
            i += 1;
            continue;
        }
        i += 1;
    }

    if let Some(path) = output_path {
        std::fs::write(Path::new(path), json).expect("write schema file");
        eprintln!("Wrote config JSON schema to {}", path);
    } else {
        io::stdout().write_all(json.as_bytes()).expect("write stdout");
    }
}

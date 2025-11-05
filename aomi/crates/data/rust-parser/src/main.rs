mod config;

use config::{ContractConfig, parse_config_file, get_handler_type_name};
use std::collections::HashMap;
use std::env;
use std::path::Path;
use walkdir::WalkDir;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() > 1 {
        // Parse specific file
        let file_path = &args[1];
        let path = Path::new(file_path);
        
        if !path.exists() {
            eprintln!("❌ File does not exist: {}", file_path);
            return;
        }
        
        if path.extension().and_then(|s| s.to_str()) != Some("jsonc") {
            eprintln!("❌ File is not a .jsonc file: {}", file_path);
            return;
        }
        
        match parse_config_file(path) {
            Ok(config) => {
                println!("✅ Successfully parsed: {}", path.display());
                println!("\n=== CONFIG CONTENT ===");
                println!("{:#?}", config);
            }
            Err(e) => {
                eprintln!("❌ Failed to parse {}: {}", path.display(), e);
            }
        }
    } else {
        // Parse all files (original behavior)
        let config_dir = "../"; // Parent directory to search for config files
        let mut config_files = Vec::new();
        let mut total_files = 0;

        // Find all .jsonc files
        for entry in WalkDir::new(config_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("jsonc"))
        {
            total_files += 1;
            let path = entry.path();
            
            match parse_config_file(path) {
                Ok(config) => {
                    config_files.push((path.to_string_lossy().to_string(), config));
                    println!("✓ Parsed: {}", path.display());
                }
                Err(e) => {
                    eprintln!("✗ Failed to parse {}: {}", path.display(), e);
                }
            }
        }

        println!("\n=== SUMMARY ===");
        println!("Total .jsonc files found: {}", total_files);
        println!("Successfully parsed: {}", config_files.len());
        println!("Failed to parse: {}", total_files - config_files.len());

        // Analyze the parsed configs
        analyze_configs(&config_files);
    }
}


fn analyze_configs(configs: &[(String, ContractConfig)]) {
    let mut schemas = HashMap::new();
    let mut categories = HashMap::new();
    let mut ignore_patterns = HashMap::new();
    let mut permission_types = HashMap::new();
    let mut handler_types = HashMap::new();

    for (_path, config) in configs {
        // Count schemas
        if let Some(schema) = &config.schema {
            *schemas.entry(schema.clone()).or_insert(0) += 1;
        }

        // Count categories
        if let Some(category) = &config.category {
            *categories.entry(category.clone()).or_insert(0) += 1;
        }

        // Count ignore patterns
        if let Some(ignore) = &config.ignore_in_watch_mode {
            for pattern in ignore {
                *ignore_patterns.entry(pattern.clone()).or_insert(0) += 1;
            }
        }

        // Count permission types
        if let Some(fields) = &config.fields {
            for field in fields.values() {
                if let Some(permissions) = &field.permissions {
                    for permission in permissions {
                        *permission_types.entry(permission.permission_type.clone()).or_insert(0) += 1;
                    }
                }
            }
        }

        // Count handler types
        if let Some(fields) = &config.fields {
            for field in fields.values() {
                if let Some(handler) = &field.handler {
                    let handler_type = get_handler_type_name(handler);
                    *handler_types.entry(handler_type.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    println!("\n=== SCHEMAS ===");
    for (schema, count) in schemas {
        println!("  {}: {} files", schema, count);
    }

    println!("\n=== CATEGORIES ===");
    let mut sorted_categories: Vec<_> = categories.into_iter().collect();
    sorted_categories.sort_by(|a, b| b.1.cmp(&a.1));
    for (category, count) in sorted_categories {
        println!("  {}: {} files", category, count);
    }

    println!("\n=== IGNORE PATTERNS ===");
    let mut sorted_patterns: Vec<_> = ignore_patterns.into_iter().collect();
    sorted_patterns.sort_by(|a, b| b.1.cmp(&a.1));
    for (pattern, count) in sorted_patterns.into_iter().take(10) {
        println!("  {}: {} files", pattern, count);
    }

    println!("\n=== PERMISSION TYPES ===");
    let mut sorted_permissions: Vec<_> = permission_types.into_iter().collect();
    sorted_permissions.sort_by(|a, b| b.1.cmp(&a.1));
    for (perm_type, count) in sorted_permissions {
        println!("  {}: {} occurrences", perm_type, count);
    }

    println!("\n=== HANDLER TYPES ===");
    let mut sorted_handlers: Vec<_> = handler_types.into_iter().collect();
    sorted_handlers.sort_by(|a, b| b.1.cmp(&a.1));
    for (handler_type, count) in sorted_handlers {
        println!("  {}: {} occurrences", handler_type, count);
    }
}

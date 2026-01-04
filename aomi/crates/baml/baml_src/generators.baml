// Generator configuration for BAML client code generation

generator target {
    // Native Rust FFI client (no HTTP server needed)
    output_type "rust"

    // Where the generated code will be saved (relative to baml_src/)
    // This creates baml_client/ as a module directory
    output_dir "../"

    // The version of the BAML package you have installed
    version "0.216.0"
}

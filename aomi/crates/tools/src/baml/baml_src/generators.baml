// Generator configuration for BAML client code generation

generator target {
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet", "rest/openapi"
    output_type "rest/openapi"

    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"

    // The version of the BAML package you have installed
    version "0.214.0"

    // 'baml-cli generate' will run this after generating openapi.yaml, to generate your OpenAPI client
    // This command will be run from within $output_dir/baml_client
    on_generate "npx @openapitools/openapi-generator-cli generate -i openapi.yaml -g rust -o . --additional-properties packageName=forge-baml-client,avoidBoxedModels=true"
}

//! Print the backup format's JSON Schema (`just gen-backup-schema` writes it
//! to `docs/backup-format.schema.json`).

fn main() {
    let schema = stacks_core::import_export::backup::format::json_schema();
    match serde_json::to_string_pretty(&schema) {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("could not serialize the schema: {e}");
            std::process::exit(1);
        }
    }
}

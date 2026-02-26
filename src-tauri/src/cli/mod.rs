pub mod pipe;

use crate::engine::whisper::WhisperEngine;
use crate::engine::SttEngine;
use crate::models::manifest;

/// List installed models and exit.
pub fn list_models() {
    let manifest = manifest::load_bundled().expect("Cannot load model manifest");
    let models_dir = models_dir_or_exit();

    let mut found = false;
    for entry in &manifest.models {
        let path = models_dir.join(&entry.file);
        if path.exists() {
            let label = format!(
                "{id:<20} {name:<25} {size}",
                id = entry.id,
                name = entry.name,
                size = format_size(entry.size_bytes),
            );
            println!("{label}");
            found = true;
        }
    }

    if !found {
        eprintln!("No models installed. Use the GUI to download a model.");
        std::process::exit(1);
    }
}

/// Set the active model by ID.
pub fn set_model(model_id: &str) {
    let manifest = manifest::load_bundled().expect("Cannot load model manifest");
    let models_dir = models_dir_or_exit();

    let entry = manifest
        .models
        .iter()
        .find(|m| m.id == model_id)
        .unwrap_or_else(|| {
            eprintln!("Unknown model: {model_id}");
            eprintln!("Use --list-models to see available models.");
            std::process::exit(1);
        });

    let model_path = models_dir.join(&entry.file);
    if !model_path.exists() {
        eprintln!("Model not downloaded: {model_id}");
        eprintln!("Use the GUI to download it first.");
        std::process::exit(1);
    }

    // Validate the model can be loaded
    eprint!("Validating model... ");
    match WhisperEngine::new(&model_path) {
        Ok(engine) => {
            eprintln!("{}", engine.name());
        }
        Err(e) => {
            eprintln!("Failed: {e}");
            std::process::exit(1);
        }
    }

    // Save to settings
    let db_path = db_path();
    let db = crate::db::open(&db_path).expect("Cannot open database");
    let conn = db.blocking_lock();
    crate::settings::set(
        &conn,
        crate::settings::keys::ACTIVE_MODEL_ID,
        &serde_json::to_string(model_id).unwrap(),
    )
    .expect("Cannot save setting");

    eprintln!("Active model set to: {model_id}");
}

fn models_dir_or_exit() -> std::path::PathBuf {
    let data_dir = dirs::data_dir().unwrap_or_else(|| {
        eprintln!("Cannot determine data directory");
        std::process::exit(1);
    });
    data_dir.join("com.echotype.app").join("models")
}

fn db_path() -> std::path::PathBuf {
    let data_dir = dirs::data_dir().expect("Cannot determine data directory");
    data_dir.join("com.echotype.app").join("echotype.db")
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.0} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    }
}

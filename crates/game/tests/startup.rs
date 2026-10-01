//! The game must refuse to start, with a clear message, when the paths to the
//! original games are missing or invalid (milestone J0).

use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn run_game_with_config(config_file: &Path, working_dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_chaos-fc"))
        .env("CHAOS_FC_CONFIG", config_file)
        .current_dir(working_dir)
        .output()
        .expect("failed to launch the game binary")
}

#[test]
fn refuses_to_start_without_config_file() {
    let tmp = TempDir::new().unwrap();
    let output = run_game_with_config(&tmp.path().join("config.toml"), tmp.path());

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ne peut pas démarrer"), "{stderr}");
    assert!(stderr.contains("config.example.toml"), "{stderr}");
}

#[test]
fn refuses_to_start_with_invalid_paths() {
    let tmp = TempDir::new().unwrap();
    let config = tmp.path().join("config.toml");
    std::fs::write(
        &config,
        "[paths]\nvice_city = 'does-not-exist'\npes6 = ''\n",
    )
    .unwrap();
    let output = run_game_with_config(&config, tmp.path());

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[paths] vice_city"), "{stderr}");
    assert!(stderr.contains("[paths] pes6"), "{stderr}");
}

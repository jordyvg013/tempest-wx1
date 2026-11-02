use std::path::PathBuf;
use std::process::Command;

#[test]
fn test_golden_header_exists_and_contains_symbols() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let header_path = manifest_dir.join("../../include/wx1.h");
    assert!(header_path.exists(), "include/wx1.h must exist");

    let content = std::fs::read_to_string(&header_path).unwrap();
    assert!(content.contains("wx1_open_file"));
    assert!(content.contains("wx1_close_file"));
    assert!(content.contains("wx1_read_array_slice"));
    assert!(content.contains("wx1_get_variable_shape"));
    assert!(content.contains("wx1_get_variable_rank"));
    assert!(content.contains("wx1_get_variable_data_type"));
    assert!(content.contains("wx1_last_error_message"));
    assert!(content.contains("wx1_error_t"));
    assert!(content.contains("wx1_data_type_t"));
}

#[test]
fn test_compile_and_run_c_integration_binary() {
    // Only run if gcc is available in PATH
    let gcc_check = Command::new("gcc").arg("--version").output();
    if gcc_check.is_err() {
        eprintln!("gcc not found, skipping standalone C binary test");
        return;
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.join("../..");

    // Ensure cdylib is built in target/debug
    let _ = Command::new("cargo")
        .arg("build")
        .arg("-p")
        .arg("wx1-ffi")
        .current_dir(&repo_root)
        .status();

    let c_source = repo_root.join("tests/c_integration/test_c_abi.c");
    let include_dir = repo_root.join("include");
    let target_dir = repo_root.join("target/debug");
    let out_binary = target_dir.join("test_c_abi_debug");

    let status = Command::new("gcc")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-Werror")
        .arg(format!("-I{}", include_dir.display()))
        .arg(c_source)
        .arg(format!("-L{}", target_dir.display()))
        .arg("-lwx1_ffi")
        .arg(format!("-Wl,-rpath,{}", target_dir.display()))
        .arg("-o")
        .arg(&out_binary)
        .arg("-lm")
        .status()
        .expect("gcc command should execute");

    assert!(
        status.success(),
        "gcc compilation of C ABI test must succeed"
    );

    // Execute the compiled C binary passing path to golden vector 1
    let gv01_path = repo_root.join("tests/fixtures/golden_vectors/gv01_valid_scalar_2d_zstd.wx1");
    let run_status = Command::new(&out_binary)
        .arg(gv01_path)
        .status()
        .expect("running C integration binary must succeed");

    assert!(
        run_status.success(),
        "C ABI integration test binary must exit with status 0"
    );

    let _ = std::fs::remove_file(&out_binary);
}

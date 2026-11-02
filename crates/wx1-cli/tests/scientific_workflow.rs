use std::fs;
use std::process::Command;

fn run_cli(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_wx1-cli"))
        .args(arguments)
        .output()
        .expect("wx1-cli process should start")
}

#[test]
fn scientific_demo_inspect_validate_and_slice_work_end_to_end() {
    let root = std::env::temp_dir().join(format!("wx1-scientific-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let dataset_path = root.join("demo.wx1");
    let slice_path = root.join("temperature-slice.raw");
    let dataset = dataset_path.to_str().unwrap();
    let slice = slice_path.to_str().unwrap();

    let created = run_cli(&["create-scientific-demo", dataset, "cli-e2e"]);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let created_text = String::from_utf8_lossy(&created.stdout);
    assert!(created_text.contains("/temperature"));
    assert!(created_text.contains("Chunks: 6"));

    let inspected = run_cli(&["inspect", dataset]);
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let inspect_text = String::from_utf8_lossy(&inspected.stdout);
    assert!(inspect_text.contains("Scientific model schema: 1"));
    assert!(inspect_text.contains("Variables: 3"));
    assert!(inspect_text
        .contains("All visible stored-byte CRC32C, BLAKE3 digests and decoded lengths validated."));

    let validated = run_cli(&["validate-scientific", dataset]);
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );
    assert!(String::from_utf8_lossy(&validated.stdout).contains("Chunks verified: 6"));

    let extracted = run_cli(&[
        "read-array-slice",
        dataset,
        "/temperature",
        "1,1",
        "2,2",
        slice,
    ]);
    assert!(
        extracted.status.success(),
        "{}",
        String::from_utf8_lossy(&extracted.stderr)
    );

    let actual = fs::read(&slice_path).unwrap();
    let expected: Vec<u8> = [285.0f32, 286.0, 289.0, 290.0]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    assert_eq!(actual, expected);

    let existing_output = run_cli(&[
        "read-array-slice",
        dataset,
        "/temperature",
        "1,1",
        "2,2",
        slice,
    ]);
    assert!(!existing_output.status.success());
    assert_eq!(fs::read(&slice_path).unwrap(), expected);

    let invalid_slice_path = root.join("invalid-slice.raw");
    let invalid_slice = run_cli(&[
        "read-array-slice",
        dataset,
        "/temperature",
        "0,not-a-number",
        "1,1",
        invalid_slice_path.to_str().unwrap(),
    ]);
    assert!(!invalid_slice.status.success());
    assert!(!invalid_slice_path.exists());

    let original_dataset = fs::read(&dataset_path).unwrap();
    let duplicate_create = run_cli(&["create-scientific-demo", dataset, "must-not-overwrite"]);
    assert!(!duplicate_create.status.success());
    assert_eq!(fs::read(&dataset_path).unwrap(), original_dataset);

    let _ = fs::remove_dir_all(&root);
}

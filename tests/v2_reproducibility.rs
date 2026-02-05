use run::v2::registry::{
    LocalRegistry, LocalRegistryConfig, PackageMetadata, Registry, RegistryConfig, compute_sha256,
};
use run::v2::registry::{LockedComponent, Lockfile};
use semver::Version;
use tempfile::tempdir;

fn minimal_wasm_bytes() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

#[test]
fn lockfile_is_deterministic_and_verifiable() {
    let mut lockfile = Lockfile::new();
    lockfile.add(LockedComponent {
        name: "b".to_string(),
        version: Version::new(1, 2, 3),
        sha256: "hash-b".to_string(),
        dependencies: vec![],
    });
    lockfile.add(LockedComponent {
        name: "a".to_string(),
        version: Version::new(0, 1, 0),
        sha256: "hash-a".to_string(),
        dependencies: vec!["b".to_string()],
    });

    let first = lockfile.serialize();
    let second = lockfile.serialize();
    assert_eq!(first, second, "lockfile serialization must be stable");

    let parsed = Lockfile::parse(&first).unwrap();
    assert!(parsed.verify(), "lockfile checksum must verify");
}

#[test]
fn registry_cache_detects_tamper_via_lockfile() {
    let temp_home = tempdir().unwrap();
    unsafe { std::env::set_var("HOME", temp_home.path()) };

    let mut local_registry = LocalRegistry::new(LocalRegistryConfig {
        registry_dir: temp_home.path().join(".run").join("registry"),
    })
    .unwrap();

    let wasm_bytes = minimal_wasm_bytes();
    let metadata = PackageMetadata {
        name: "acme:calc".to_string(),
        version: "1.0.0".to_string(),
        description: "Test component".to_string(),
        sha256: String::new(),
        dependencies: vec![],
        license: Some("Apache-2.0".to_string()),
        repository: None,
        wit: None,
        published_at: 0,
    };

    local_registry
        .publish("acme:calc", &Version::new(1, 0, 0), &wasm_bytes, metadata)
        .unwrap();

    let project_dir = tempdir().unwrap();
    let mut registry = Registry::new(RegistryConfig::default(), project_dir.path()).unwrap();
    registry.load_lockfile().unwrap();

    let _ = futures::executor::block_on(async {
        registry
            .install("acme:calc", Some("=1.0.0"), Default::default())
            .await
            .unwrap()
    });

    let cache_dir = project_dir.path().join(".run").join("cache");
    let cached_path = cache_dir.join("acme__calc@1.0.0.wasm");
    std::fs::write(&cached_path, b"tampered").unwrap();

    let invalid = registry.verify_all().unwrap();
    assert_eq!(invalid, vec!["acme:calc".to_string()]);

    let expected = compute_sha256(&wasm_bytes);
    assert_ne!(expected, compute_sha256(b"tampered"));
}

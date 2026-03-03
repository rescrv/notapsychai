use std::fs;
use std::path::PathBuf;

use orgremode::parse;

#[test]
fn vectors_roundtrip() {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.push("tests");
    root.push("vectors");

    let mut files: Vec<PathBuf> = fs::read_dir(&root)
        .expect("read tests/vectors")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext == "orglike")
        })
        .collect();

    files.sort();
    assert!(
        !files.is_empty(),
        "no .orglike vectors found in tests/vectors"
    );

    for path in files {
        let input = fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!("failed to read {}: {}", path.display(), err);
        });

        let doc = parse(&input).unwrap_or_else(|err| {
            panic!("parse failed for {}: {}", path.display(), err);
        });

        let rendered = format!("{}", doc);
        let roundtrip = parse(&rendered).unwrap_or_else(|err| {
            panic!("roundtrip parse failed for {}: {}", path.display(), err);
        });

        assert_eq!(doc, roundtrip, "roundtrip mismatch for {}", path.display());
    }
}

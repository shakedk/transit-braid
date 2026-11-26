//! The golden harness: the digests of everything the pipeline draws for every committed
//! fixture must equal `fixtures/golden.json` (the format is in `src/golden.rs`). A refactor that
//! changes nothing passes as is; a change to the drawing fails here, naming each digest that
//! changed. To accept a change on purpose, rewrite the file and review its diff:
//!
//! ```sh
//! BRAID_GOLDEN_UPDATE=1 cargo test -p transit-braid-testkit --test golden
//! ```

use std::io::Write as _;
use std::path::Path;

use transit_braid_testkit::golden::{GoldenFile, record_all};
use transit_braid_testkit::load_dir;

/// The committed fixture folders (city fixtures are local only: `transit-braid-cli golden`).
const FOLDERS: [&str; 3] = ["synthetic", "bus", "stress"];

/// What the file says about itself.
const ABOUT: &str = "Golden digests of every committed fixture (format: \
    crates/transit-braid-testkit/src/golden.rs). Checked natively by `cargo test -p \
    transit-braid-testkit --test golden` and in WebAssembly by packages/transit-braid/src/\
    golden.test.ts. Update only on purpose: BRAID_GOLDEN_UPDATE=1 cargo test -p \
    transit-braid-testkit --test golden, then review the diff.";

fn digest_committed_fixtures(root: &Path) -> GoldenFile {
    let mut fixtures = Vec::new();
    for folder in FOLDERS {
        let found = load_dir(&root.join(folder)).unwrap_or_else(|e| panic!("{e}"));
        assert!(!found.is_empty(), "no fixtures in fixtures/{folder}");
        fixtures.extend(found);
    }
    let records = record_all(root, &fixtures).unwrap_or_else(|e| panic!("{e}"));
    GoldenFile::new(ABOUT, records)
}

#[test]
fn committed_fixtures_draw_exactly_their_golden_digests() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let path = root.join("golden.json");
    let now = digest_committed_fixtures(&root);

    if std::env::var("BRAID_GOLDEN_UPDATE").is_ok_and(|v| v == "1") {
        let old = std::fs::read_to_string(&path).map(|t| GoldenFile::parse(&t));
        let text = now.to_json().unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut changed = match old {
            Ok(Ok(old)) => now.differences(&old),
            Ok(Err(e)) => vec![format!("(the old file was unreadable: {e})")],
            Err(_) => vec!["(a new file)".to_owned()],
        };
        if changed.is_empty() {
            changed.push("(no digest changed)".to_owned());
        }
        // Straight to stderr, past the test harness's capture, so the list always shows.
        let _ = writeln!(
            std::io::stderr(),
            "BRAID_GOLDEN_UPDATE=1: rewrote {}:\n  {}",
            path.display(),
            changed.join("\n  ")
        );
        return;
    }

    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}\nCreate it on purpose: BRAID_GOLDEN_UPDATE=1 cargo test -p \
             transit-braid-testkit --test golden",
            path.display()
        )
    });
    let expected = GoldenFile::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let changed = now.differences(&expected);
    assert!(
        changed.is_empty(),
        "\n\nTHE DRAWING CHANGED: {} golden digests differ from {}:\n  {}\n\n\
         If the change is on purpose, rewrite the file and review its diff:\n  \
         BRAID_GOLDEN_UPDATE=1 cargo test -p transit-braid-testkit --test golden\n",
        changed.len(),
        path.display(),
        changed.join("\n  ")
    );
}

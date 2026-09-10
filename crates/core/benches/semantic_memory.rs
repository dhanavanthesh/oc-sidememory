mod support;

use support::{array_documents, array_schema, compiled, guide};

fn main() {
    let documents = array_documents(1_000);
    let compiled = compiled(
        &array_schema(
            1_000,
            r#","uniqueItems":true,"contains":{"type":"string"},"minContains":1"#,
        ),
        &documents,
    );
    for depth in [0, 1, 8, 32, 128] {
        let mut guide = guide(compiled.clone(), depth);
        if depth != 0 {
            guide.advance(0).expect("advance succeeds");
        }
        println!(
            "rollback_depth={depth} available={} retained_rollback_bytes={}",
            guide.rollback_available(),
            guide.retained_rollback_bytes()
        );
    }
}

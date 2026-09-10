mod support;

use support::{array_documents, array_schema, compiled, guide, measure, print_row};

fn main() {
    for size in [3, 100, 1_000] {
        let documents = array_documents(size);
        for (name, suffix) in [
            ("none", ""),
            ("unique", r#","uniqueItems":true"#),
            (
                "contains",
                r#","contains":{"type":"string"},"minContains":1"#,
            ),
            (
                "combined",
                r#","uniqueItems":true,"contains":{"type":"string"},"minContains":1"#,
            ),
        ] {
            let compiled = compiled(&array_schema(size, suffix), &documents);
            let mut guide = guide(compiled, 32);
            let mut mask = vec![0_u32; guide.model_width().div_ceil(32)];
            let result = measure(|_| {
                guide.write_mask(&mut mask).expect("mask succeeds");
                mask.iter()
                    .fold(0_u64, |digest, word| digest ^ u64::from(*word))
            });
            print_row(&format!("mask/{name}/{size}"), result);
        }
    }
}

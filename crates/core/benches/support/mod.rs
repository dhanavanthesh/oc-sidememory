#![allow(dead_code)]

use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use oc_sidememory::{
    compile_schema, CompileOptions, CompiledSchema, Guide, GuideOptions, Vocabulary,
};

pub fn compiled(schema: &str, documents: &[String]) -> Arc<CompiledSchema> {
    let eos = documents.len() as u32;
    let mut vocabulary = Vocabulary::new(eos);
    for (token, document) in documents.iter().enumerate() {
        vocabulary
            .try_insert(document.as_bytes(), token as u32)
            .expect("benchmark vocabulary is valid");
    }
    Arc::new(
        compile_schema(
            schema.as_bytes(),
            &vocabulary,
            documents.len() + 1,
            &CompileOptions::default(),
        )
        .expect("benchmark schema compiles"),
    )
}

pub fn array_documents(size: usize) -> Vec<String> {
    (0..size).map(|index| format!(r#"["v{index}"]"#)).collect()
}

pub fn array_schema(size: usize, semantic: &str) -> String {
    let values = (0..size)
        .map(|index| format!(r#""v{index}""#))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"type":"array","items":{{"type":"string","enum":[{values}]}},"minItems":1,"maxItems":1{semantic}}}"#
    )
}

pub fn guide(compiled: Arc<CompiledSchema>, rollback: usize) -> Guide {
    Guide::new(
        compiled,
        GuideOptions {
            max_rollback_tokens: rollback,
            ..GuideOptions::default()
        },
    )
    .expect("benchmark guide builds")
}

pub fn measure(mut operation: impl FnMut(usize) -> u64) -> (u128, u128, u128, u64) {
    const WARMUP: usize = 32;
    const SAMPLES: usize = 256;
    for sample in 0..WARMUP {
        black_box(operation(sample));
    }
    let mut times = Vec::with_capacity(SAMPLES);
    let mut digest = 0_u64;
    for sample in 0..SAMPLES {
        let start = Instant::now();
        digest ^= black_box(operation(sample)).rotate_left((sample % 63) as u32);
        times.push(start.elapsed().as_nanos());
    }
    times.sort_unstable();
    let percentile = |p: usize| times[(SAMPLES * p).div_ceil(100).saturating_sub(1)];
    (times[SAMPLES / 2], percentile(95), percentile(99), digest)
}

pub fn print_row(case: &str, result: (u128, u128, u128, u64)) {
    println!(
        "case={case} p50_ns={} p95_ns={} p99_ns={} digest={}",
        result.0, result.1, result.2, result.3
    );
}

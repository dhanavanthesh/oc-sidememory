#![allow(dead_code)]

use std::sync::Arc;

use oc_sidememory::{
    compile_schema, CompileOptions, CompiledSchema, ExtensionPlanV1, Guide, GuideOptions,
    Vocabulary,
};

pub fn compile(schema: &str, documents: &[&[u8]], extension: Option<&str>) -> Arc<CompiledSchema> {
    let eos = documents.len() as u32;
    let mut vocabulary = Vocabulary::new(eos);
    for (token, document) in documents.iter().enumerate() {
        vocabulary
            .try_insert(document.to_vec(), token as u32)
            .expect("example vocabulary is valid");
    }
    let options = CompileOptions {
        extension_plan: extension.map(|text| {
            ExtensionPlanV1::from_json(text).expect("example extension plan is valid")
        }),
        ..CompileOptions::default()
    };
    Arc::new(
        compile_schema(schema.as_bytes(), &vocabulary, documents.len() + 1, &options)
            .expect("example schema compiles"),
    )
}

pub fn guide(compiled: Arc<CompiledSchema>) -> Guide {
    Guide::new(compiled, GuideOptions::default()).expect("example guide builds")
}

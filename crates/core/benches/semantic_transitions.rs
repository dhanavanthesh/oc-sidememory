mod support;

use support::{array_documents, array_schema, compiled, guide, measure, print_row};

fn main() {
    let documents = array_documents(100);
    let compiled = compiled(
        &array_schema(
            100,
            r#","uniqueItems":true,"contains":{"type":"string"},"minContains":1"#,
        ),
        &documents,
    );
    let mut probe_guide = guide(compiled.clone(), 128);
    print_row(
        "transition/probe",
        measure(|sample| u64::from(probe_guide.probe((sample % 100) as u32).is_ok())),
    );
    print_row(
        "transition/advance",
        measure(|sample| {
            let mut guide = guide(compiled.clone(), 128);
            guide
                .advance((sample % 100) as u32)
                .expect("advance succeeds");
            guide.rollback_available() as u64
        }),
    );
    let mut rollback_guides = (0..288)
        .map(|sample| {
            let mut guide = guide(compiled.clone(), 128);
            guide
                .advance((sample % 100) as u32)
                .expect("advance succeeds");
            guide
        })
        .collect::<Vec<_>>();
    let mut index = 0;
    print_row(
        "transition/rollback",
        measure(|_| {
            rollback_guides[index]
                .rollback(1)
                .expect("rollback succeeds");
            index += 1;
            rollback_guides[index - 1].rollback_available() as u64
        }),
    );
}

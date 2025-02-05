#[expect(dead_code)]
#[expect(non_camel_case_types)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bind-cxlmalloc.rs"));
}

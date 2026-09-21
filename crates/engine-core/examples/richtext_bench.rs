//! Reproducible synthetic benchmark, not a DOCX upload or pagination benchmark.
use engine_core::richtext;
use serde_json::{json, Value};
use std::time::Instant;

fn p(text: &str) -> Value {
    json!({"type":"paragraph","content":[{"type":"text","text":text}]})
}
fn median(mut f: impl FnMut()) -> f64 {
    f();
    let mut runs = Vec::new();
    for _ in 0..9 {
        let start = Instant::now();
        f();
        runs.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    runs.sort_by(f64::total_cmp);
    runs[4]
}
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--fixtures") {
        let before = p("বাংলা 👩🏽‍🔬 e\u{301} العربية");
        let mut after = p("বাংলা 👩🏽‍🔬 revised e\u{301} العربية");
        after["content"][0]["marks"] = json!([{"type":"bold"},{"type":"textStyle","attrs":{"fontFamily":"Noto Sans","color":"#005bbb","unknown":true}}]);
        let nested_before = json!({"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":[before.clone()]}]}]});
        let nested_after = json!({"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","content":[after.clone()]}]}]});
        let mut heading = before.clone();
        heading["attrs"] = json!({"textAlign":"center","blockId":"stable"});
        let cases:Vec<_>=[(before.clone(),after),(nested_before,nested_after),(before.clone(),heading),(before,json!({"type":"paragraph","content":[]}))].into_iter().map(|(before,after)|json!({"patch":richtext::make_patch(&before,&after).unwrap(),"before":before,"after":after})).collect();
        println!("{}", serde_json::to_string_pretty(&cases).unwrap());
        return;
    }
    let words =
        "Research participants described their experience and reviewed the study findings. "
            .repeat(2);
    let blocks:Vec<_>=(0..8000).map(|i|json!({"type":if i%20==0 {"heading"} else {"paragraph"},"attrs":{"blockId":format!("fixture-{i}"),"level":2},"content":[{"type":"text","text":words},{"type":"text","text":words,"marks":[{"type":"bold"}]},{"type":"text","text":words,"marks":[{"type":"textStyle","attrs":{"fontFamily":"Arial","color":"#005bbb"}}]}]})).collect();
    let document = json!({"type":"doc","content":blocks});
    let bytes = serde_json::to_vec(&document).unwrap();
    let classification_ms = median(|| {
        std::hint::black_box(richtext::analyze(&document));
    });
    let parse_ms = median(|| {
        std::hint::black_box(serde_json::from_slice::<Value>(&bytes).unwrap());
    });
    let before = p(&words.repeat(60));
    let mut after = before.clone();
    after["content"][0]["marks"] = json!([{"type":"bold"}]);
    let patch = richtext::make_patch(&before, &after).unwrap();
    let patch_ms = median(|| {
        std::hint::black_box(richtext::make_patch(&before, &after).unwrap());
    });
    let replay_ms = median(|| {
        std::hint::black_box(richtext::apply_patch(&before, &patch).unwrap());
    });
    let b = p("The participants were interviewed in June.");
    let t = p("The respondents were interviewed in June.");
    let d = p("The participants were interviewed in July.");
    let merge_ms = median(|| {
        std::hint::black_box(richtext::merge(&b, &t, &d).unwrap());
    });
    println!("{}",serde_json::to_string_pretty(&json!({"fixture":"synthetic 8,000 paragraphs/headings; page analogy only (20 blocks/page)","arch":std::env::consts::ARCH,"os":std::env::consts::OS,"debug_assertions":cfg!(debug_assertions),"samples":9,"json_bytes":bytes.len(),"analysis":richtext::analyze(&document),"median_ms":{"parse_json":parse_ms,"classify":classification_ms,"format_patch":patch_ms,"replay_patch":replay_ms,"merge_paragraph":merge_ms},"long_paragraph":{"whole_content_json_bytes":serde_json::to_vec(&after).unwrap().len(),"patch_json_bytes":serde_json::to_vec(&patch).unwrap().len()},"excludes":["DOCX unzip/OOXML conversion","upload/network","database/event writes","editor rendering","page layout","multiple concurrent requests"]})).unwrap());
}

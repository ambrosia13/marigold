use std::cell::LazyCell;

use regex::bytes::Regex;

thread_local! {
    static ENTRYPOINT_REGEX: LazyCell<Regex> = LazyCell::new(|| {
        Regex::new(r#"\[\[shader\("(\w+)"\)]]\s*\w+\s+(\w+)\s*\("#).unwrap()
    });
}

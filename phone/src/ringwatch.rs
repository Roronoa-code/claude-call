use serde_json::Value;

pub fn state_url(link: &str, v: i64) -> Option<String> {
    let (base, query) = link.split_once("/?").filter(|_| link.starts_with("https://"))?;
    Some(format!("{base}/state?{query}&ring=1&v={v}"))
}

pub fn taken(st: &Value) -> bool {
    st["answered"] == true || st["declined"] == true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watches_the_call_link_without_counting_as_the_call_page() {
        assert_eq!(state_url("https://x.trycloudflare.com/?t=k&call=1", -1).as_deref(), Some("https://x.trycloudflare.com/state?t=k&call=1&ring=1&v=-1"));
        assert_eq!(state_url("test:ring", -1), None);
    }

    #[test]
    fn another_device_answering_or_declining_stops_the_ring() {
        assert!(taken(&serde_json::json!({ "answered": true })));
        assert!(taken(&serde_json::json!({ "declined": true })));
        assert!(!taken(&serde_json::json!({ "answered": false, "declined": false, "v": 3 })));
        assert!(!taken(&serde_json::json!({ "v": 3 })));
    }
}

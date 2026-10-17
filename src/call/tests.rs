use super::*;

fn request(method: Method, path: &str) -> Request {
    Request::builder().method(method).uri(path).body(Body::empty()).unwrap()
}

async fn state(response: Response) -> serde_json::Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 16_384).await.unwrap()).unwrap()
}

#[tokio::test]
async fn ringing_devices_observe_answer_and_decline_without_becoming_call_pages() {
    assert!(crate::CONFIG.set(crate::Config {
        topic: "ring-test".into(),
        token: "ring-test".into(),
        status_path: std::env::temp_dir().join("callme-ring-test-status.json"),
        test_status: true,
        local: true,
        no_gpu: true,
        agent: "test".into(),
    }).is_ok());

    let unauthorized = handle(request(Method::GET, "/state?t=wrong&ring=1")).await.unwrap();
    assert_eq!(unauthorized.status(), StatusCode::NOT_FOUND);
    let initial = state(handle(request(Method::GET, "/state?t=ring-test&ring=1")).await.unwrap()).await;
    assert_eq!(initial["answered"], false);
    assert_eq!(initial["declined"], false);

    for (version, action, flag) in [(0, "answer", "answered"), (1, "decline", "declined")] {
        let mut poll = Box::pin(handle(request(Method::GET, &format!("/state?t=ring-test&ring=1&v={version}"))));
        assert!(timeout(Duration::from_millis(20), &mut poll).await.is_err());
        assert_eq!(call().watchers, 0);
        assert!(call().last_seen.is_none());
        assert_eq!(handle(request(Method::POST, &format!("/{action}?t=ring-test"))).await.unwrap().status(), StatusCode::NO_CONTENT);
        let update = state(timeout(Duration::from_secs(1), poll).await.expect("ring poll did not wake").unwrap()).await;
        assert_eq!(update[flag], true);
        assert_eq!(update["v"], version + 1);
    }

    let mut page = Box::pin(handle(request(Method::GET, "/state?t=ring-test&v=2")));
    assert!(timeout(Duration::from_millis(20), &mut page).await.is_err());
    assert_eq!(call().watchers, 1);
    drop(page);
    assert_eq!(call().watchers, 0);
    assert!(call().last_seen.is_some());
}

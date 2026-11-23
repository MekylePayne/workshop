//! Integration test: the native Pages deployment flow against a local mock
//! implementing the documented endpoints. Verifies the request shapes, the
//! auth split (scoped API token for account/deployment requests, upload JWT
//! for asset requests), the manifest field, and progress reporting - the
//! full TL-03 contract minus the real network.

use hsw_cloudflare::pages::{CloudflareClient, DeployStep};


use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Response, Server};

#[derive(Debug, Clone)]
struct Recorded {
    method: String,
    path: String,
    auth: String,
    body: String,
}

fn start_mock(log: Arc<Mutex<Vec<Recorded>>>) -> (u16, Arc<AtomicBool>) {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Arc::new(Server::from_listener(listener, None).unwrap());
    let stop = Arc::new(AtomicBool::new(false));

    let stop_thread = stop.clone();
    std::thread::spawn(move || loop {
        if stop_thread.load(Ordering::SeqCst) {
            break;
        }
        let mut request = match server.recv() {
            Ok(request) => request,
            Err(_) => break,
        };
        if stop_thread.load(Ordering::SeqCst) {
            break;
        }
        let method = request.method().to_string();
        let path = request.url().split('?').next().unwrap_or("").to_string();
        let auth = request
            .headers()
            .iter()
            .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case("authorization"))
            .map(|header| header.value.as_str().to_string())
            .unwrap_or_default();
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);

        let payload = route(&method, &path, &body);
        log.lock().unwrap().push(Recorded { method, path, auth, body });
        let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
        let response = Response::from_data(payload.into_bytes()).with_header(header);
        let _ = request.respond(response);
    });
    (port, stop)
}

fn ok(result: &str) -> String {
    format!("{{\"success\":true,\"result\":{result},\"errors\":[]}}")
}

fn route(method: &str, path: &str, body: &str) -> String {
    match (method, path) {
        ("GET", "/accounts") => ok(r#"[{"id":"acc1","name":"Test Account"}]"#),
        ("GET", "/accounts/acc1/pages/projects") => ok(r#"[{"name":"site","subdomain":"site.pages.dev"}]"#),
        ("GET", "/accounts/acc1/pages/projects/site/upload-token") => ok(r#""jwt-token-1""#),
        ("POST", "/accounts/acc1/pages/assets/check-missing") => {
            // Report every hash except the first as missing, so exactly one
            // asset goes through the upload step.
            let hashes: Vec<String> = serde_json::from_str(body).unwrap_or_default();
            let missing: Vec<String> = hashes.into_iter().skip(1).collect();
            ok(&serde_json::json!(missing).to_string())
        }
        ("POST", "/pages/assets/upload") => ok(r#"[]"#),
        ("POST", "/accounts/acc1/pages/assets/upsert-hashes") => ok(r#"{}"#),
        ("POST", "/accounts/acc1/pages/projects/site/deployments") => ok(r#"{"url":"https://site.pages.dev"}"#),
        ("GET", "/accounts/acc1/workers/scripts") => ok(r#"[{"id":"old-worker","modified_on":"2026-01-01"}]"#),
        ("PUT", "/accounts/acc1/workers/scripts/new-worker") => ok(r#"{"id":"new-worker"}"#),
        ("DELETE", "/accounts/acc1/workers/scripts/old-worker") => ok(r#"{}"#),
        _ => "{\"success\":false,\"errors\":[{\"message\":\"no route\"}]}".to_string(),
    }
}

fn fixture_project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("image")).unwrap();
    std::fs::write(tmp.path().join("index.html"), "<h1>Deployed</h1>").unwrap();
    std::fs::write(tmp.path().join("image/logo.png"), b"png-bytes").unwrap();
    tmp
}

#[tokio::test]
async fn full_pages_deployment_flow_against_the_mock() {
    let log = Arc::new(Mutex::new(Vec::<Recorded>::new()));
    let (port, stop) = start_mock(log.clone());
    let client = CloudflareClient::with_api_root("test-token", format!("http://127.0.0.1:{port}"));

    let tmp = fixture_project();
    let steps = Arc::new(Mutex::new(Vec::<(DeployStep, usize, usize)>::new()));
    let sink = steps.clone();
    let outcome = client
        .deploy_directory(
            "acc1",
            "site",
            tmp.path(),
            &move |step: DeployStep, done: usize, total: usize| {
                sink.lock().unwrap().push((step, done, total));
            },
        )
        .await
        .expect("deployment must succeed against the mock");

    assert_eq!(outcome.total_files, 2);
    assert_eq!(outcome.uploaded + outcome.already_present, 2);
    assert_eq!(outcome.url.as_deref(), Some("https://site.pages.dev"));

    let records = log.lock().unwrap();
    let token_call = records.iter().find(|r| r.path.ends_with("/upload-token")).unwrap();
    assert_eq!(token_call.auth, "Bearer test-token");
    let check_call = records.iter().find(|r| r.path.ends_with("/check-missing")).unwrap();
    assert_eq!(check_call.auth, "Bearer jwt-token-1", "asset endpoints use the upload JWT");
    let expected_hash = hsw_cloudflare::hashing::asset_hash(b"<h1>Deployed</h1>");
    assert!(check_call.body.contains(&expected_hash), "check-missing sends the content hashes");
    let upload_call = records.iter().find(|r| r.path == "/pages/assets/upload").unwrap();
    assert_eq!(upload_call.auth, "Bearer jwt-token-1");
    assert!(upload_call.body.contains("\"base64\":true"));
    assert!(upload_call.body.contains("text/html"), "content type metadata included");
    let deploy_call = records
        .iter()
        .find(|r| r.path.ends_with("/projects/site/deployments"))
        .unwrap();
    assert_eq!(deploy_call.auth, "Bearer test-token");
    assert!(deploy_call.body.contains("manifest"), "multipart body contains the manifest field");
    assert!(deploy_call.body.contains("index.html"));
    let upsert_call = records.iter().find(|r| r.path.ends_with("/upsert-hashes")).unwrap();
    assert!(upsert_call.body.contains("\"hashes\""));

    let recorded_steps = steps.lock().unwrap();
    assert!(recorded_steps.iter().any(|(step, _, _)| *step == DeployStep::Hashing));
    assert!(recorded_steps.iter().any(|(step, _, _)| *step == DeployStep::Done));

    stop.store(true, Ordering::SeqCst);
}

#[tokio::test]
async fn accounts_and_projects_are_listed() {
    let log = Arc::new(Mutex::new(Vec::<Recorded>::new()));
    let (port, stop) = start_mock(log);
    let client = CloudflareClient::with_api_root("test-token", format!("http://127.0.0.1:{port}"));

    let accounts = client.accounts().await.unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "acc1");

    let projects = client.pages_projects("acc1").await.unwrap();
    assert_eq!(projects[0].name, "site");
    assert_eq!(projects[0].subdomain.as_deref(), Some("site.pages.dev"));

    stop.store(true, Ordering::SeqCst);
}

#[tokio::test]
async fn workers_scripts_list_upload_and_delete() {
    let log = Arc::new(Mutex::new(Vec::<Recorded>::new()));
    let (port, stop) = start_mock(log.clone());
    let client = CloudflareClient::with_api_root("test-token", format!("http://127.0.0.1:{port}"));

    let scripts = client.workers_scripts("acc1").await.unwrap();
    assert_eq!(scripts[0].id, "old-worker");

    client
        .upload_worker_script(
            "acc1",
            "new-worker",
            "index.js",
            "export default { fetch() { return new Response('ok'); } }",
            "[]",
        )
        .await
        .unwrap();
    {
        let records = log.lock().unwrap();
        let put = records
            .iter()
            .find(|r| r.method == "PUT" && r.path.ends_with("/workers/scripts/new-worker"))
            .expect("PUT must be recorded");
        assert!(put.body.contains("main_module") || put.body.contains("index.js"));
        assert!(put.body.contains("export default"));
    }

    client.delete_worker_script("acc1", "old-worker").await.unwrap();
    {
        let records = log.lock().unwrap();
        assert!(records
            .iter()
            .any(|r| r.method == "DELETE" && r.path.ends_with("/workers/scripts/old-worker")));
    }
    stop.store(true, Ordering::SeqCst);
}

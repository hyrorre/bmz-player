use super::test_support::*;
use super::*;
use std::net::{SocketAddr, TcpListener};

// Separate the policy's DNS answers from the loopback-only test transport.
// Only hosts which passed the public-address policy receive a transport override.
fn fixture_dns() -> FixtureResolver {
    let answers = [
        ("source.test".into(), vec!["8.8.8.8:0".parse().unwrap()]),
        ("cdn.test".into(), vec!["1.1.1.1:0".parse().unwrap()]),
        ("local.test".into(), vec!["127.0.0.1:0".parse().unwrap()]),
        ("proxy.test".into(), vec!["127.0.0.1:0".parse().unwrap()]),
    ]
    .into_iter()
    .collect();
    FixtureResolver(answers)
}

fn public_services(hosts: &[&str]) -> DownloadServices {
    DownloadServices {
        timeout: Some(Duration::from_secs(2)),
        resolver: Some(Arc::new(fixture_dns())),
        address_overrides: hosts
            .iter()
            .map(|host| ((*host).into(), vec!["127.0.0.1:0".parse().unwrap()]))
            .collect(),
        ..Default::default()
    }
}

fn public_url(url: &str, host: &str) -> String {
    let mut url = Url::parse(url).unwrap();
    url.set_host(Some(host)).unwrap();
    url.to_string()
}

#[tokio::test]
async fn public_redirects_and_metadata_cannot_connect_to_local_destinations() {
    for metadata in [false, true] {
        for host in [
            "127.0.0.1",
            "local.test",
            "[::ffff:127.0.0.1]",
            "localhost",
            "LOCALHOST.",
            "child.localhost.",
            "node.local.",
        ] {
            let target = TcpListener::bind("127.0.0.1:0").unwrap();
            target.set_nonblocking(true).unwrap();
            let destination = format!(
                "http://{host}:{}/private?token=target-secret",
                target.local_addr().unwrap().port()
            );
            let source = HttpFixture::raw(
                move |_| {
                    if metadata {
                        let body = serde_json::json!({"downloadURL": destination}).to_string();
                        format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes()
                    } else {
                        format!("HTTP/1.1 302 Found\r\nLocation: {destination}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
                    }
                },
                Duration::ZERO,
            );
            let root = tempfile::tempdir().unwrap();
            let api =
                format!("{}api/{{md5}}?key=api-secret", public_url(&source.url, "source.test"));
            let error = download_chart_controlled(
                request(root.path(), &api, 1),
                Arc::default(),
                Arc::new(public_services(&["source.test"])),
                None,
            )
            .await
            .unwrap_err();
            let error = format!("{error:#}");
            assert!(error.contains("local"), "metadata={metadata}, host={host}: {error}");
            assert!(!error.contains("api-secret") && !error.contains("target-secret"), "{error}");
            assert_eq!(source.requests.load(Ordering::Relaxed), 1);
            assert_eq!(target.accept().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
        }
    }
}

#[tokio::test]
async fn public_cross_origin_redirects_still_download_archives() {
    let chart = b"#TITLE Public download\n#BPM 120\n#00011:01\n";
    let cdn = HttpFixture::new(archive(&[("song/chart.bms", chart)]), Duration::ZERO, 200);
    let destination = public_url(&cdn.url, "cdn.test");
    let source = HttpFixture::raw(
        move |_| {
            format!("HTTP/1.1 302 Found\r\nLocation: {destination}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
        },
        Duration::ZERO,
    );
    let root = tempfile::tempdir().unwrap();
    let result = download_chart_controlled(
        request(root.path(), &format!("{}{{md5}}", public_url(&source.url, "source.test")), 1),
        Arc::default(),
        Arc::new(public_services(&["source.test", "cdn.test"])),
        None,
    )
    .await
    .unwrap();
    assert!(result.scan_path.join("song/chart.bms").is_file());
    assert_eq!(source.requests.load(Ordering::Relaxed), 1);
    assert_eq!(cdn.requests.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn explicitly_configured_local_api_keeps_metadata_downloads_working() {
    let body = archive(&[("song/chart.bms", b"#TITLE Local\n#BPM 120\n")]);
    let server = HttpFixture::with_handler(
        move |path| {
            if path == "/archive" {
                (200, "application/octet-stream".into(), body.clone())
            } else {
                (200, "application/json".into(), br#"{"downloadURL":"/archive"}"#.to_vec())
            }
        },
        Duration::ZERO,
    );
    let root = tempfile::tempdir().unwrap();
    let result =
        download_chart(request(root.path(), &format!("{}/{{md5}}", server.url), 1)).await.unwrap();
    assert!(result.scan_path.join("song/chart.bms").is_file());
    assert_eq!(server.requests.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn mixed_public_and_local_initial_dns_is_rejected() {
    let resolver = FixtureResolver(
        [(
            "source.test".into(),
            vec!["8.8.8.8:0".parse::<SocketAddr>().unwrap(), "127.0.0.1:0".parse().unwrap()],
        )]
        .into_iter()
        .collect(),
    );
    assert!(
        network::DownloadResolver::new(
            &Url::parse("https://source.test/api").unwrap(),
            Arc::new(resolver),
            Duration::from_secs(3),
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn configured_system_proxy_fixture_child() {
    let Ok(mode) = std::env::var("BMZ_TEST_DOWNLOAD_PROXY") else { return };
    let api = std::env::var("BMZ_TEST_DOWNLOAD_PROXY_API").unwrap();
    let mut services = public_services(&["source.test"]);
    services.use_system_proxy = true;
    let mut dns = fixture_dns();
    if std::env::var_os("BMZ_TEST_DOWNLOAD_PROXY_DNS_FAILURE").is_some() {
        dns.0.remove("source.test");
    }
    if mode.starts_with("success_") {
        dns.0.remove("cdn.test");
    }
    services.resolver = Some(Arc::new(dns));
    let root = tempfile::tempdir().unwrap();
    let result = download_chart_controlled(
        request(root.path(), &api, 1),
        Arc::default(),
        Arc::new(services),
        None,
    )
    .await;
    if mode.starts_with("success") {
        assert!(result.unwrap().scan_path.join("song/chart.bms").is_file());
    } else {
        assert!(format!("{:#}", result.unwrap_err()).contains("local"));
    }
}

#[test]
fn system_proxy_is_preserved_but_local_redirects_and_metadata_never_reach_it() {
    // Child-only environment changes avoid racing other tests' proxy settings.
    // A transport override also traps accidental direct connections on loopback.
    for (mode, unresolved) in [false, true].into_iter().flat_map(|unresolved| {
        ["success", "success_redirect", "success_metadata", "redirect", "metadata"]
            .into_iter()
            .map(move |mode| (mode, unresolved))
    }) {
        let direct = HttpFixture::new(Vec::new(), Duration::ZERO, 500);
        let destination = format!(
            "http://local.test:{}/private",
            Url::parse(&direct.url).unwrap().port().unwrap()
        );
        let body = archive(&[("song/chart.bms", b"#TITLE Proxy\n#BPM 120\n")]);
        let cdn = public_url(&direct.url, "cdn.test");
        let proxy = HttpFixture::raw_request(
            move |request| {
                assert!(
                    request.starts_with("GET http://source.test:")
                        || request.starts_with("GET http://cdn.test:"),
                    "{request}"
                );
                match mode {
                    "redirect" => redirect(302, &destination),
                    "success_redirect" if request.starts_with("GET http://source.test:") => {
                        redirect(302, &cdn)
                    }
                    "metadata" | "success_metadata"
                        if request.starts_with("GET http://source.test:") =>
                    {
                        let target = if mode == "metadata" { &destination } else { &cdn };
                        let body = serde_json::json!({"downloadURL": target}).to_string();
                        format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes()
                    }
                    _ => {
                        let mut response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
                        response.extend_from_slice(&body);
                        response
                    }
                }
            },
            Duration::ZERO,
        );
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "song_download::network_tests::configured_system_proxy_fixture_child",
                "--nocapture",
            ])
            .env("BMZ_TEST_DOWNLOAD_PROXY", mode)
            .env(
                "BMZ_TEST_DOWNLOAD_PROXY_API",
                format!("{}{{md5}}", public_url(&direct.url, "source.test")),
            )
            .env("HTTP_PROXY", public_url(&proxy.url, "proxy.test"));
        if unresolved {
            command.env("BMZ_TEST_DOWNLOAD_PROXY_DNS_FAILURE", "1");
        } else {
            command.env_remove("BMZ_TEST_DOWNLOAD_PROXY_DNS_FAILURE");
        }
        for name in ["NO_PROXY", "no_proxy", "ALL_PROXY", "all_proxy"] {
            command.env_remove(name);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = if mode.starts_with("success_") { 2 } else { 1 };
        assert_eq!(
            proxy.requests.load(Ordering::Relaxed),
            expected,
            "{mode}, unresolved={unresolved}"
        );
        assert_eq!(direct.requests.load(Ordering::Relaxed), 0, "{mode}");
    }
}

fn redirect(status: u16, destination: &str) -> Vec<u8> {
    format!("HTTP/1.1 {status} Redirect\r\nLocation: {destination}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
}

async fn local_network(url: &str, timeout: Duration) -> Arc<network::DownloadResolver> {
    network::DownloadResolver::new(
        &Url::parse(url).unwrap(),
        Arc::new(network::SystemResolver),
        timeout,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn redirects_preserve_methods_referer_and_same_origin_basic_auth() {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    for status in [301, 302, 303, 307, 308] {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = seen.clone();
        let server = HttpFixture::raw_request(
            move |request| {
                captured.lock().unwrap().push(request.to_owned());
                if request.split_whitespace().nth(1).unwrap() == "/start" {
                    redirect(status, "/next")
                } else {
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".to_vec()
                }
            },
            Duration::ZERO,
        );
        let mut url = Url::parse(&format!("{}/start#fragment", server.url)).unwrap();
        url.set_username("user").unwrap();
        url.set_password(Some("pass")).unwrap();
        let network = local_network(url.as_str(), Duration::from_secs(3)).await;
        assert_eq!(
            network
                .request(&client, reqwest::Method::POST, url)
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "ok"
        );
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert!(seen[0].starts_with("POST "));
        assert!(seen[1].starts_with(if status >= 307 { "POST " } else { "GET " }));
        for request in seen.iter() {
            assert!(request.to_ascii_lowercase().contains("authorization: basic dxnlcjpwyxnz"));
        }
        let second = seen[1].to_ascii_lowercase();
        assert!(second.contains(&format!("referer: {}/start", server.url)));
        assert!(!second.contains("user:pass") && !second.contains("#fragment"));
    }
}

#[tokio::test]
async fn cross_origin_redirects_strip_basic_auth_and_stop_after_ten_hops() {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let target = HttpFixture::raw_request(
        |request| {
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
        },
        Duration::ZERO,
    );
    let destination = target.url.clone();
    let source = HttpFixture::raw(move |_| redirect(302, &destination), Duration::ZERO);
    let url = Url::parse(&source.url.replace("http://", "http://user:pass@")).unwrap();
    local_network(url.as_str(), Duration::from_secs(3))
        .await
        .request(&client, reqwest::Method::GET, url)
        .await
        .unwrap();
    assert_eq!(target.requests.load(Ordering::Relaxed), 1);
    for last in [10, 11] {
        let server = HttpFixture::raw(
            move |path| {
                let hop: usize = path.trim_start_matches('/').parse().unwrap();
                if hop == last {
                    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
                } else {
                    redirect(302, &format!("/{}", hop + 1))
                }
            },
            Duration::ZERO,
        );
        let url = Url::parse(&format!("{}/0", server.url)).unwrap();
        let result = local_network(url.as_str(), Duration::from_secs(3))
            .await
            .request(&client, reqwest::Method::GET, url)
            .await;
        assert_eq!(result.is_ok(), last == 10);
        assert_eq!(server.requests.load(Ordering::Relaxed), 11);
    }
}

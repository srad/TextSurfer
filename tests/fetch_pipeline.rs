use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use textsurf::app::App;
use textsurf::app::net::{Navigate, PoolNet};
use textsurf::net::{FetchPool, FileFetch, SchemeFetch, UreqFetch};

const BODY: &str = "<!doctype html><title>smoke</title><p>acceptance</p>";

fn serve_once(
    handler: impl FnOnce(&mut std::net::TcpStream) + Send + 'static,
) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback bind");
    let addr = listener.local_addr().expect("local addr");
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            handler(&mut stream);
            break;
        }
    });
    addr
}

fn real_app() -> (App, Arc<SchemeFetch>) {
    let fetch = Arc::new(SchemeFetch {
        http: Arc::new(UreqFetch::new()),
        file: Arc::new(FileFetch),
    });
    let net: Arc<dyn Navigate> =
        Arc::new(PoolNet::new(Arc::new(FetchPool::spawn(fetch.clone(), 4))));
    (App::with_net(net), fetch)
}

fn wait_loaded(app: &mut App, deadline_ms: u64) {
    let deadline = Instant::now() + Duration::from_millis(deadline_ms);
    loop {
        app.step(Instant::now());
        let content = app.chrome_view().content.lines.clone();
        if content
            .first()
            .is_some_and(|line| line.starts_with("#document"))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the parsed tree"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn real_pipeline_loads_a_page_over_loopback_http() {
    let addr = serve_once(|stream| {
        let mut reader = std::io::BufReader::new(&mut *stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).unwrap();
        assert!(request_line.starts_with("GET / HTTP/1.1"));
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{}",
            BODY.len(),
            BODY
        )
        .unwrap();
    });
    let (mut app, _fetch) = real_app();
    app.submit_url(&format!("http://{addr}/"));
    wait_loaded(&mut app, 5000);
    let tab = app.chrome_view();
    assert!(
        tab.content
            .lines
            .iter()
            .any(|line| line.contains("acceptance"))
    );
    assert!(
        tab.content
            .lines
            .iter()
            .any(|line| line.contains("<title>"))
    );
    assert!(app.message().contains("accepted gen 1"));
}

#[test]
fn real_pipeline_serves_files() {
    let path = std::env::temp_dir().join(format!("textsurf-e2e-{}.html", std::process::id()));
    std::fs::write(&path, BODY).expect("write fixture");
    let url = url::Url::from_file_path(&path).expect("file url");
    let (mut app, _fetch) = real_app();
    app.submit_url(url.as_str());
    wait_loaded(&mut app, 5000);
    let tab = app.chrome_view();
    assert!(
        tab.content
            .lines
            .iter()
            .any(|line| line.contains("acceptance"))
    );
    assert!(app.message().contains("accepted gen 1"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn real_pipeline_shows_http_errors_as_a_page() {
    let addr = serve_once(|stream| {
        let mut drain = [0u8; 512];
        let _ = stream.read(&mut drain);
        write!(
            stream,
            "HTTP/1.1 410 Gone\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
    });
    let (mut app, _fetch) = real_app();
    app.submit_url(&format!("http://{addr}/gone"));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.step(Instant::now());
        let content = app.chrome_view().content.lines.clone();
        if content
            .first()
            .is_some_and(|line| line.starts_with("failed to load"))
        {
            break;
        }
        assert!(Instant::now() < deadline, "error page never rendered");
        thread::sleep(Duration::from_millis(10));
    }
    assert!(app.message().contains("http status 410"));
}

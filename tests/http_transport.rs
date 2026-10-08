use canopy_desktop::{http::new_client, integrations::github::Github};
use futures_lite::{future::block_on, io::AsyncReadExt};
use gpui_kit::http_client::{AsyncBody, HttpRequestExt, RedirectPolicy, Request};
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

fn accept(listener: TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                return stream;
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("HTTP test server did not receive a connection: {error}"),
        }
    }
}
fn read_headers(stream: &mut TcpStream) -> String {
    let mut reader = BufReader::new(stream);
    let mut headers = String::new();
    loop {
        let mut line = String::new();
        assert_ne!(reader.read_line(&mut line).unwrap(), 0);
        headers.push_str(&line);
        if line == "\r\n" {
            return headers;
        }
    }
}

#[test]
fn desktop_transport_sends_headers_and_preserves_no_follow() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut stream = accept(listener);
        let headers = read_headers(&mut stream).to_ascii_lowercase();
        assert!(headers.starts_with("get /probe http/1.1"));
        assert!(headers.contains("authorization: bearer canopy-test-value\r\n"));
        assert!(headers.contains("user-agent: canopy-desktop\r\n"));
        stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /must-not-follow\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let client = new_client().unwrap();
    let mut authorization =
        gpui_kit::http_client::http::HeaderValue::from_static("Bearer canopy-test-value");
    authorization.set_sensitive(true);
    let request = Request::get(format!("http://{address}/probe"))
        .header("Authorization", authorization)
        .follow_redirects(RedirectPolicy::NoFollow)
        .timeout(Duration::from_secs(2))
        .body(AsyncBody::empty())
        .unwrap();
    let response = block_on(client.send(request)).unwrap();
    assert_eq!(response.status(), 302);
    server.join().unwrap();
}

#[test]
fn desktop_transport_applies_request_deadline_to_response_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut stream = accept(listener);
        read_headers(&mut stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n")
            .unwrap();
        thread::sleep(Duration::from_millis(400));
        let _ = stream.write_all(b"x");
    });
    let client = new_client().unwrap();
    let request = Request::get(format!("http://{address}/probe"))
        .timeout(Duration::from_millis(150))
        .body(AsyncBody::empty())
        .unwrap();
    let mut response = block_on(client.send(request)).unwrap();
    assert!(block_on(response.body_mut().read_to_end(&mut Vec::new())).is_err());
    server.join().unwrap();
}

#[test]
#[ignore = "Contacts api.github.com over HTTPS with a deliberately invalid token; no user credentials"]
fn live_github_returns_authentication_response_through_desktop_transport() {
    let client = new_client().unwrap();
    let request = Request::get("https://api.github.com/")
        .follow_redirects(RedirectPolicy::NoFollow)
        .timeout(Duration::from_secs(20))
        .body(AsyncBody::empty())
        .unwrap();
    let response = block_on(client.send(request)).unwrap();
    assert_eq!(response.status(), 200);
    let mut body = Vec::new();
    block_on(
        response
            .into_body()
            .take(4 * 1024 * 1024)
            .read_to_end(&mut body),
    )
    .unwrap();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&body)
            .unwrap()
            .is_object()
    );
    let github = Github::new(client, "canopy-deliberately-invalid-transport-probe".into());
    let error = block_on(github.verify()).unwrap_err();
    assert!(error.starts_with("GitHub rejected the token."), "{error}");
}

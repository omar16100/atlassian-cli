//! `get_text` and `get_bytes` when the response body does not arrive in full.
//!
//! `bb pipeline logs` failed at times on a large log with "error decoding
//! response body", 30 seconds after the start. The probable cause is a body
//! that stops in the middle (not verified on a real site). Two things make
//! that a failure of the command. The 30 second timeout of the client is a
//! limit for the full request, the body included. And an error in the body
//! read was not retried, although the request is a GET.
//!
//! wiremock sends a response in one piece, so these use a small server on a
//! socket that can stop in the middle of a body.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::ApiError;
use crate::retry::RetryConfig;
use crate::ApiClient;

const BODY: &str = "line one of the log\nline two of the log\n";

/// What the server does with one connection, in the order of the connections.
#[derive(Clone, Copy)]
enum Reply {
    /// The full body.
    Full,
    /// Half of the body, then the connection closes.
    CutAndClose,
    /// Half of the body, then nothing for the given time.
    CutAndStall(Duration),
    /// The body in two halves, with a pause between them.
    Slow(Duration),
}

fn read_request(stream: &mut TcpStream) {
    let mut seen = Vec::new();
    let mut buffer = [0u8; 1024];
    while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(n) => seen.extend_from_slice(&buffer[..n]),
        }
    }
}

/// Start the server. Returns its base URL and a counter of the requests.
fn serve(replies: Vec<Reply>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    thread::spawn(move || {
        for reply in replies {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            seen.fetch_add(1, Ordering::SeqCst);
            thread::spawn(move || {
                read_request(&mut stream);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    BODY.len()
                );
                let (first, second) = BODY.as_bytes().split_at(BODY.len() / 2);
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(first);
                let _ = stream.flush();
                match reply {
                    Reply::Full => {
                        let _ = stream.write_all(second);
                    }
                    Reply::CutAndClose => {}
                    Reply::CutAndStall(time) => thread::sleep(time),
                    Reply::Slow(pause) => {
                        thread::sleep(pause);
                        let _ = stream.write_all(second);
                    }
                }
            });
        }
    });
    (base, count)
}

fn fast_retries() -> RetryConfig {
    RetryConfig {
        max_retries: 3,
        initial_interval: Duration::from_millis(10),
        max_interval: Duration::from_millis(20),
        multiplier: 1.0,
    }
}

/// The reported case, without the wait: the body stops, and a second try
/// gives the full log.
#[tokio::test]
async fn get_text_tries_again_when_the_body_is_cut() {
    let (base, requests) = serve(vec![Reply::CutAndClose, Reply::Full]);
    let client = ApiClient::new(&base)
        .unwrap()
        .with_retry_config(fast_retries());

    let text = client
        .get_text("/log")
        .await
        .expect("the second try gives the log");

    assert_eq!(text, BODY);
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn get_bytes_tries_again_when_the_body_is_cut() {
    let (base, requests) = serve(vec![Reply::CutAndClose, Reply::Full]);
    let client = ApiClient::new(&base)
        .unwrap()
        .with_retry_config(fast_retries());

    let bytes = client
        .get_bytes("/file")
        .await
        .expect("the second try gives the file");

    assert_eq!(bytes, BODY.as_bytes());
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

/// When each try fails, the error says what failed, and the tries stop at the
/// configured number.
#[tokio::test]
async fn a_body_that_is_cut_each_time_is_an_error_after_the_last_try() {
    let (base, requests) = serve(vec![Reply::CutAndClose; 3]);
    let client = ApiClient::new(&base)
        .unwrap()
        .with_retry_config(fast_retries());

    let err = client.get_text("/log").await.unwrap_err();

    assert!(matches!(err, ApiError::BodyReadFailed(_)), "{err:?}");
    assert!(
        err.to_string().contains("response body"),
        "the error names the body: {err}"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 3);
}

/// A body that stops and does not close the connection: the read timeout ends
/// the try, and the next try gives the log. Before, the command waited for the
/// full request timeout and then failed.
#[tokio::test]
async fn get_text_tries_again_when_the_body_stalls() {
    let (base, requests) = serve(vec![
        Reply::CutAndStall(Duration::from_secs(5)),
        Reply::Full,
    ]);
    let client = ApiClient::new_with_timeouts(
        &base,
        crate::HttpTimeouts {
            read: Duration::from_millis(200),
            ..crate::HttpTimeouts::default()
        },
    )
    .unwrap()
    .with_retry_config(fast_retries());

    let started = Instant::now();
    let text = client
        .get_text("/log")
        .await
        .expect("the second try gives the log");

    assert_eq!(text, BODY);
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the stall must end at the read timeout: {:?}",
        started.elapsed()
    );
}

/// A download can take longer than the timeout of an ordinary request, as
/// long as the data continues to arrive.
#[tokio::test]
async fn a_download_that_continues_is_not_cut_by_the_request_timeout() {
    let (base, requests) = serve(vec![Reply::Slow(Duration::from_millis(600))]);
    let client = ApiClient::new_with_timeouts(
        &base,
        crate::HttpTimeouts {
            request: Duration::from_millis(300),
            read: Duration::from_secs(5),
            download: Duration::from_secs(10),
        },
    )
    .unwrap()
    .with_retry_config(fast_retries());

    let text = client
        .get_text("/log")
        .await
        .expect("the slow body is complete");

    assert_eq!(text, BODY);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

/// An ordinary request keeps its timeout: only downloads get more time.
#[tokio::test]
async fn an_ordinary_request_keeps_the_request_timeout() {
    let (base, _) = serve(vec![Reply::Slow(Duration::from_secs(3))]);
    let client = ApiClient::new_with_timeouts(
        &base,
        crate::HttpTimeouts {
            request: Duration::from_millis(300),
            read: Duration::from_secs(5),
            download: Duration::from_secs(10),
        },
    )
    .unwrap()
    .with_retry_config(RetryConfig {
        max_retries: 1,
        ..fast_retries()
    });

    let started = Instant::now();
    let result: Result<serde_json::Value, _> = client.get("/json").await;

    // The pause of the server is 3 seconds. An answer before that can only
    // come from the request timeout.
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_millis(2500));
}

#[tokio::test]
async fn get_bytes_tries_again_when_the_body_stalls() {
    let (base, requests) = serve(vec![
        Reply::CutAndStall(Duration::from_secs(5)),
        Reply::Full,
    ]);
    let client = ApiClient::new_with_timeouts(
        &base,
        crate::HttpTimeouts {
            read: Duration::from_millis(200),
            ..crate::HttpTimeouts::default()
        },
    )
    .unwrap()
    .with_retry_config(fast_retries());

    let bytes = client
        .get_bytes("/file")
        .await
        .expect("the second try gives the file");

    assert_eq!(bytes, BODY.as_bytes());
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

/// `request_raw` takes a timeout from its caller (`api --timeout`). The read
/// timeout must not cut a request that waits longer than it for the headers.
#[tokio::test]
async fn request_raw_can_wait_longer_than_the_read_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        read_request(&mut stream);
        thread::sleep(Duration::from_millis(600));
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    });
    let client = ApiClient::new_with_timeouts(
        &base,
        crate::HttpTimeouts {
            request: Duration::from_millis(200),
            read: Duration::from_millis(200),
            download: Duration::from_secs(10),
        },
    )
    .unwrap();

    let response = client
        .request_raw(crate::RawRequest {
            method: reqwest::Method::GET,
            path: "/slow",
            headers: reqwest::header::HeaderMap::new(),
            body: None,
            timeout: Some(Duration::from_secs(5)),
        })
        .await
        .expect("the caller's timeout applies, not the read timeout");

    assert_eq!(response.status, 200);
}

/// The error text must not have the URL: after a redirect it is the address
/// of the file service.
#[tokio::test]
async fn the_body_error_does_not_show_the_url() {
    let (base, _) = serve(vec![Reply::CutAndClose]);
    let client = ApiClient::new(&base)
        .unwrap()
        .with_retry_config(RetryConfig {
            max_retries: 1,
            ..fast_retries()
        });

    let err = client.get_text("/secret-path").await.unwrap_err();

    assert!(!err.to_string().contains("secret-path"), "{err}");
    assert!(!err.to_string().contains("127.0.0.1"), "{err}");
}

#[test]
fn a_body_read_failure_is_retryable_and_has_a_hint() {
    let err = ApiError::BodyReadFailed("error decoding response body".to_string());
    assert!(err.is_retryable());
    assert!(err.suggestion().is_some());
    // A body that arrived and cannot be parsed is not retried.
    assert!(!ApiError::InvalidResponse("bad".to_string()).is_retryable());
}

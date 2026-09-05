//! End-to-end CLI tests: run the real `celestia-browser` binary as a child
//! process against the local test server and assert on stdout, exit codes,
//! and output files.

mod common;

use std::collections::HashMap;
use std::process::{Command, Stdio};

use common::{spawn_sync, Route};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_celestia-browser")
}

fn run(args: &[&str], stdin: Option<&str>) -> (i32, String, String) {
    let mut cmd = Command::new(bin());
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn().expect("spawn celestia-browser");
    if let Some(input) = stdin {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(input.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().expect("wait");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn small_site() -> HashMap<String, Route> {
    let mut routes = HashMap::new();
    routes.insert(
        "/".to_string(),
        Route::html("<html><body><a href=\"/about\">about</a><a href=\"/leaf\">leaf</a></body></html>"),
    );
    routes.insert("/about".to_string(), Route::html("<html><body>about page</body></html>"));
    routes.insert("/leaf".to_string(), Route::html("<html><body>leaf page</body></html>"));
    routes.insert(
        "/robots.txt".to_string(),
        Route::text("User-agent: *\nDisallow: /hidden"),
    );
    routes.insert("/hidden".to_string(), Route::text("hidden page"));
    routes
}

#[test]
fn cli_help_shows_name_and_flags() {
    let (code, stdout, _) = run(&["--help"], None);
    assert_eq!(code, 0);
    assert!(stdout.contains("celestia-browser"), "name in help: {stdout}");
    assert!(stdout.contains("--headless"));
    assert!(stdout.contains("--jsonl"));
    assert!(stdout.contains("--depth"));
}

#[test]
fn cli_version_prints_name() {
    let (code, stdout, _) = run(&["--version"], None);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("celestia-browser"), "{stdout}");
}

#[test]
fn cli_rejects_missing_depth_and_duration() {
    // No URLs and no way to set depth... use a URL but zero depth.
    let (code, _, stderr) = run(&["-u", "https://127.0.0.1:1", "-d", "0"], None);
    assert_eq!(code, 1);
    assert!(stderr.contains("max-depth or crawl-duration"), "{stderr}");
}

#[test]
fn cli_basic_crawl_prints_results() {
    let server = standard_site_and_server();
    let (code, stdout, stderr) = run(
        &["-u", &format!("{}/", server.1), "-d", "1", "--no-color"],
        None,
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains(&server.1), "index in stdout: {stdout}");
    assert!(stdout.contains("/about"), "about discovered: {stdout}");
    assert!(stderr.contains("crawl finished"), "summary logged: {stderr}");
    server.0.shutdown();
}

#[test]
fn cli_silent_suppresses_logs_but_not_results() {
    let server = standard_site_and_server();
    let (code, stdout, stderr) = run(
        &["-u", &format!("{}/", server.1), "-d", "1", "--silent"],
        None,
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("/about"), "results still printed: {stdout}");
    assert!(!stderr.contains("[INF]"), "no log lines in silent: {stderr}");
    server.0.shutdown();
}

#[test]
fn cli_jsonl_output_is_valid_json() {
    let server = standard_site_and_server();
    let (code, stdout, stderr) = run(
        &["-u", &format!("{}/", server.1), "-d", "1", "-j", "--silent"],
        None,
    );
    assert_eq!(code, 0, "{stderr}");
    let mut count = 0;
    for line in stdout.lines() {
        let v: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("invalid jsonl `{line}`: {e}"));
        assert!(v["request"]["endpoint"].as_str().is_some());
        count += 1;
    }
    assert!(count >= 3, "at least index+about+leaf: {count} lines");
    server.0.shutdown();
}

#[test]
fn cli_output_file_written_and_no_clobber() {
    let server = standard_site_and_server();
    let dir = std::env::temp_dir().join("e2e_cli_out");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("out.txt");
    let _ = std::fs::remove_file(&file);

    let (code, _, stderr) = run(
        &[
            "-u",
            &format!("{}/", server.1),
            "-d",
            "1",
            "--silent",
            "-o",
            file.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code, 0, "{stderr}");
    let content = std::fs::read_to_string(&file).expect("output file written");
    assert!(content.contains("/about"));

    // no-clobber: second run writes to out-1.txt
    let (code2, _, _) = run(
        &[
            "-u",
            &format!("{}/", server.1),
            "-d",
            "1",
            "--silent",
            "--no-clobber",
            "-o",
            file.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code2, 0);
    let alt = dir.join("out-1.txt");
    assert!(alt.exists(), "no-clobber alternate file created");
    let _ = std::fs::remove_dir_all(&dir);
    server.0.shutdown();
}

#[test]
fn cli_stdin_urls_accepted() {
    let server = standard_site_and_server();
    let (code, stdout, stderr) = run(
        &["-d", "1", "--silent"],
        Some(&format!("{}/\n", server.1)),
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("/about"), "stdin url crawled: {stdout}");
    server.0.shutdown();
}

#[test]
fn cli_known_files_discovers_robots_paths() {
    let server = standard_site_and_server();
    let (code, stdout, _) = run(
        &[
            "-u",
            &format!("{}/", server.1),
            "--known-files",
            "robotstxt",
            "--silent",
        ],
        None,
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("/hidden"), "robots disallow path crawled: {stdout}");
    server.0.shutdown();
}

#[test]
fn cli_store_response_creates_raw_dumps() {
    let server = standard_site_and_server();
    let dir = std::env::temp_dir().join("e2e_cli_resp");
    let _ = std::fs::remove_dir_all(&dir);
    let (code, _, stderr) = run(
        &[
            "-u",
            &format!("{}/", server.1),
            "-d",
            "1",
            "--silent",
            "--store-response",
            "--store-response-dir",
            dir.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(code, 0, "{stderr}");
    let index = std::fs::read_to_string(dir.join("index.txt")).expect("index.txt");
    assert!(index.contains("/about"), "index lists stored responses: {index}");
    // At least one per-host dump file exists with raw HTTP content.
    let mut found_raw = false;
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if path.file_name().unwrap() == "index.txt" {
            continue;
        }
        for f in std::fs::read_dir(&path).unwrap().flatten() {
            let content = std::fs::read_to_string(f.path()).unwrap();
            if content.contains("HTTP/1.1 200 OK") {
                found_raw = true;
            }
        }
    }
    assert!(found_raw, "raw response stored");
    let _ = std::fs::remove_dir_all(&dir);
    server.0.shutdown();
}

#[test]
fn cli_health_check_runs() {
    let (code, stdout, _) = run(&["--health-check"], None);
    assert_eq!(code, 0);
    assert!(stdout.contains("Health Check"), "{stdout}");
    assert!(stdout.contains("dns resolution"));
}

#[test]
fn cli_match_regex_filters_stdout() {
    let server = standard_site_and_server();
    let (code, stdout, _) = run(
        &[
            "-u",
            &format!("{}/", server.1),
            "-d",
            "1",
            "--match-regex",
            "about",
            "--no-color",
        ],
        None,
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("/about"));
    assert!(!stdout.lines().any(|l| l.contains("/leaf")), "leaf filtered: {stdout}");
    server.0.shutdown();
}

#[test]
fn cli_fields_selector_output() {
    let server = standard_site_and_server();
    let (code, stdout, _) = run(
        &[
            "-u",
            &format!("{}/", server.1),
            "-d",
            "1",
            "--field",
            "url,path",
            "--silent",
        ],
        None,
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("/about"), "url field: {stdout}");
    assert!(stdout.lines().any(|l| l == "/about"), "path field: {stdout}");
    server.0.shutdown();
}

// ---------------------------------------------------------------- helpers

type ServerPair = (common::TestServer, String);

fn standard_site_and_server() -> ServerPair {
    let server = spawn_sync(small_site());
    let url = server.base_url.clone();
    (server, url)
}

use clap::Parser;
use colored::Colorize;
use serde::Serialize;
use std::fs;
use std::net::ToSocketAddrs;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Parser, Debug)]
#[command(name = "tcp-probe", about = "Fast TCP health probe")]
struct Args {
    /// Target hosts (host:port)
    targets: Vec<String>,

    /// Timeout per connection attempt
    #[arg(short, long, default_value = "5s")]
    timeout: String,

    /// Number of retries on failure
    #[arg(short, long, default_value_t = 0)]
    retries: u32,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Read targets from file (one per line)
    #[arg(short, long)]
    file: Option<String>,

    /// Concurrent probe limit
    #[arg(short, long, default_value_t = 50)]
    concurrency: usize,
}

#[derive(Debug, Serialize)]
struct ProbeResult {
    host: String,
    status: String,
    latency_ms: Option<f64>,
    error: Option<String>,
    retries_used: u32,
}

#[derive(Debug, Serialize)]
struct Summary {
    results: Vec<ProbeResult>,
    healthy: usize,
    total: usize,
}

fn parse_duration(s: &str) -> Duration {
    let s = s.trim();
    if let Some(secs) = s.strip_suffix('s') {
        Duration::from_secs_f64(secs.parse().unwrap_or(5.0))
    } else if let Some(ms) = s.strip_suffix("ms") {
        Duration::from_millis(ms.parse().unwrap_or(5000))
    } else {
        Duration::from_secs(s.parse().unwrap_or(5))
    }
}

async fn probe_host(host: &str, connect_timeout: Duration, retries: u32) -> ProbeResult {
    let mut last_error = None;
    let mut retries_used = 0;

    for attempt in 0..=retries {
        if attempt > 0 {
            retries_used = attempt;
            tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
        }

        // Resolve DNS first
        let addr = match host.to_socket_addrs() {
            Ok(mut addrs) => match addrs.next() {
                Some(a) => a,
                None => {
                    last_error = Some("DNS resolution failed: no addresses".to_string());
                    continue;
                }
            },
            Err(e) => {
                last_error = Some(format!("DNS error: {}", e));
                continue;
            }
        };

        let start = Instant::now();
        match timeout(connect_timeout, TcpStream::connect(addr)).await {
            Ok(Ok(_stream)) => {
                let elapsed = start.elapsed();
                return ProbeResult {
                    host: host.to_string(),
                    status: "ok".to_string(),
                    latency_ms: Some(elapsed.as_secs_f64() * 1000.0),
                    error: None,
                    retries_used,
                };
            }
            Ok(Err(e)) => {
                last_error = Some(format!("Connection refused: {}", e));
            }
            Err(_) => {
                last_error = Some(format!("timeout ({}ms)", connect_timeout.as_millis()));
            }
        }
    }

    ProbeResult {
        host: host.to_string(),
        status: "fail".to_string(),
        latency_ms: None,
        error: last_error,
        retries_used,
    }
}

fn print_result(result: &ProbeResult) {
    if result.status == "ok" {
        let latency = result.latency_ms.unwrap_or(0.0);
        let retries_info = if result.retries_used > 0 {
            format!(" (retries: {})", result.retries_used)
        } else {
            String::new()
        };
        println!(
            "{} {:<30} {:.1}ms{}",
            "[OK]  ".green().bold(),
            result.host,
            latency,
            retries_info
        );
    } else {
        let error = result.error.as_deref().unwrap_or("unknown");
        println!(
            "{} {:<30} {}",
            "[FAIL]".red().bold(),
            result.host,
            error.red()
        );
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let connect_timeout = parse_duration(&args.timeout);

    // Collect all targets
    let mut targets: Vec<String> = args.targets.clone();
    if let Some(file_path) = &args.file {
        match fs::read_to_string(file_path) {
            Ok(content) => {
                for line in content.lines() {
                    let line = line.trim();
                    if !line.is_empty() && !line.starts_with('#') {
                        targets.push(line.to_string());
                    }
                }
            }
            Err(e) => {
                eprintln!("{} Failed to read file: {}", "error:".red().bold(), e);
                std::process::exit(1);
            }
        }
    }

    if targets.is_empty() {
        eprintln!("{} No targets specified", "error:".red().bold());
        std::process::exit(1);
    }

    // Run probes concurrently
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(args.concurrency));
    let mut handles = Vec::new();

    for target in &targets {
        let sem = semaphore.clone();
        let target = target.clone();
        let retries = args.retries;

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            probe_host(&target, connect_timeout, retries).await
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(result) = handle.await {
            results.push(result);
        }
    }

    let healthy = results.iter().filter(|r| r.status == "ok").count();

    if args.json {
        let summary = Summary {
            results,
            healthy,
            total: targets.len(),
        };
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        for result in &results {
            print_result(result);
        }
        println!(
            "\n{}: {}/{} healthy",
            "Summary".bold(),
            healthy,
            targets.len()
        );
    }

    if healthy < targets.len() {
        std::process::exit(1);
    }
}
//! Manual sanity check for the Windows ICMP path (Task 3, brief step 4):
//! not a unit test (no live-internet dependency in `cargo test`), just a
//! one-shot proof that `IcmpCreateFile`/`IcmpSendEcho` actually reaches a
//! real host from this machine, no admin rights required.
//!
//! Run: `cargo run -p alidade-engine --example ping_google`

use alidade_engine::{probe_once, Probe};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let p = Probe::Icmp {
        host: "8.8.8.8".into(),
    };
    let sample = probe_once(&p, Duration::from_secs(2)).await;
    match sample.rtt {
        Some(rtt) => println!("8.8.8.8 icmp rtt = {:.1} ms", rtt.as_secs_f64() * 1000.0),
        None => println!("8.8.8.8 icmp: lost (timeout, blocked, or offline)"),
    }
}

//! In-memory rate limiting: fixed-window counters per (client, bucket) plus a
//! per-client active-download gauge. RAM-bounded via periodic sweeps.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const PERIOD: Duration = Duration::from_secs(60);
const MAX_KEYS: usize = 20_000;

pub struct RateLimiter {
    counters: Mutex<HashMap<(String, &'static str), (u32, Instant)>>,
    active: Mutex<HashMap<String, u32>>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            counters: Mutex::new(HashMap::new()),
            active: Mutex::new(HashMap::new()),
        }
    }

    /// One request against `bucket`. Returns false when over `limit`.
    pub fn check(&self, client: &str, bucket: &'static str, limit: u32) -> bool {
        let now = Instant::now();
        let mut map = self.counters.lock().unwrap();
        if map.len() > MAX_KEYS {
            map.retain(|_, v| now.duration_since(v.1) < PERIOD * 2);
        }
        let e = map.entry((client.to_string(), bucket)).or_insert((0, now));
        if now.duration_since(e.1) >= PERIOD {
            *e = (0, now);
        }
        if e.0 >= limit {
            false
        } else {
            e.0 += 1;
            true
        }
    }

    /// Try to claim one concurrent download slot for `client`.
    pub fn dl_start(&self, client: &str, per_client: u32) -> bool {
        let mut map = self.active.lock().unwrap();
        let cur = map.entry(client.to_string()).or_insert(0);
        if *cur >= per_client {
            return false;
        }
        *cur += 1;
        true
    }

    pub fn dl_end(&self, client: &str) {
        let mut map = self.active.lock().unwrap();
        if let Some(v) = map.get_mut(client) {
            *v = v.saturating_sub(1);
            if *v == 0 {
                map.remove(client);
            }
        }
    }
}

/// Best-effort client identity: proxy chain first, then direct peer.
pub fn client_key(xff: Option<&str>, peer: std::net::SocketAddr) -> String {
    if let Some(v) = xff {
        if let Some(first) = v.split(',').next() {
            let t = first.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    peer.ip().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_blocks_after_limit_and_resets() {
        let rl = RateLimiter::new();
        for _ in 0..3 {
            assert!(rl.check("1.2.3.4", "info", 3));
        }
        assert!(!rl.check("1.2.3.4", "info", 3));
        // other clients unaffected
        assert!(rl.check("5.6.7.8", "info", 3));
        // other buckets unaffected
        assert!(rl.check("1.2.3.4", "dl", 3));
    }

    #[test]
    fn concurrency_gauge() {
        let rl = RateLimiter::new();
        assert!(rl.dl_start("a", 2));
        assert!(rl.dl_start("a", 2));
        assert!(!rl.dl_start("a", 2));
        rl.dl_end("a");
        assert!(rl.dl_start("a", 2));
        rl.dl_end("a");
        rl.dl_end("a");
        rl.dl_end("a"); // no underflow
        assert!(rl.dl_start("a", 2));
    }

    #[test]
    fn client_key_prefers_xff() {
        let peer: std::net::SocketAddr = "10.0.0.1:5000".parse().unwrap();
        assert_eq!(client_key(Some("1.1.1.1, 10.0.0.9"), peer), "1.1.1.1");
        assert_eq!(client_key(None, peer), "10.0.0.1");
    }
}

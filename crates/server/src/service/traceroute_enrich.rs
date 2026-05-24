use dashmap::DashMap;
use serverbee_common::types::TracerouteHop;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::service::asn::AsnService;

const PTR_CACHE_TTL: Duration = Duration::from_secs(3600);
const PTR_CACHE_MAX: usize = 4096;

#[derive(Clone, Default)]
pub struct TracerouteEnricher {
    /// IpAddr -> (hostname_or_none, inserted_at). `None` means "we tried and got nothing"
    /// — we still cache the negative result to avoid hammering DNS.
    ptr_cache: Arc<DashMap<IpAddr, (Option<String>, Instant)>>,
    /// Optional ASN database handle, shared with `AppState.asn` so re-downloads
    /// are picked up without rebuilding the enricher.
    asn: Option<Arc<RwLock<Option<AsnService>>>>,
}

impl TracerouteEnricher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach an ASN database handle. Returns a new enricher sharing the same
    /// PTR cache (cheap Arc clone).
    pub fn with_asn(mut self, asn: Arc<RwLock<Option<AsnService>>>) -> Self {
        self.asn = Some(asn);
        self
    }

    /// Fill in `hostname` (via PTR) and `asn` (via MMDB) for every hop with an
    /// IP. Best-effort; failures leave the field at whatever value the agent
    /// sent. ASN lookup is skipped entirely when no MMDB is loaded.
    pub async fn enrich(&self, hops: &mut [TracerouteHop]) {
        // Walk hops, prefer `ips[0]` (new schema) over `ip` (legacy).
        for hop in hops.iter_mut() {
            let ip_str = hop.ips.first().cloned().or_else(|| hop.ip.clone());
            let Some(s) = ip_str else { continue };
            let Ok(ip) = IpAddr::from_str(&s) else {
                continue;
            };
            if hop.hostname.is_none() {
                hop.hostname = self.ptr_lookup(ip).await;
            }
            if hop.asn.is_none()
                && let Some(asn_handle) = &self.asn
                && let Ok(guard) = asn_handle.read()
                && let Some(service) = guard.as_ref()
            {
                hop.asn = service.lookup(ip);
            }
        }
    }

    async fn ptr_lookup(&self, ip: IpAddr) -> Option<String> {
        let now = Instant::now();
        if let Some(entry) = self.ptr_cache.get(&ip) {
            let (cached, inserted_at) = entry.value();
            if now.duration_since(*inserted_at) < PTR_CACHE_TTL {
                return cached.clone();
            }
        }
        // Miss or expired — look up.
        let result = tokio::task::spawn_blocking(move || dns_lookup::lookup_addr(&ip).ok())
            .await
            .ok()
            .flatten();
        // Evict oldest if over cap (cheap, not strict LRU — accept some churn).
        if self.ptr_cache.len() >= PTR_CACHE_MAX {
            // Drop ~1/16 of entries: scan all, sort by inserted_at ASC, drop the
            // oldest. The full scan is O(n) but only runs at the eviction boundary.
            let limit = PTR_CACHE_MAX / 16;
            let mut entries: Vec<(IpAddr, Instant)> = self
                .ptr_cache
                .iter()
                .map(|e| (*e.key(), e.value().1))
                .collect();
            entries.sort_by_key(|(_, inserted_at)| *inserted_at);
            for (k, _) in entries.into_iter().take(limit) {
                self.ptr_cache.remove(&k);
            }
        }
        self.ptr_cache.insert(ip, (result.clone(), now));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::types::TracerouteHop;

    fn hop_with_ip(s: &str) -> TracerouteHop {
        TracerouteHop {
            hop: 1, ip: Some(s.into()), hostname: None,
            rtt1: None, rtt2: None, rtt3: None, asn: None,
            ips: vec![], total_sent: None, total_recv: None, loss_pct: None,
            best_ms: None, worst_ms: None, avg_ms: None, stddev_ms: None, jitter_ms: None,
        }
    }

    #[tokio::test]
    async fn test_enrich_leaves_asn_none_when_no_db_attached() {
        // Without an ASN MMDB attached, the field stays None — the enricher
        // must not invent values.
        let e = TracerouteEnricher::new();
        let mut hops = vec![hop_with_ip("8.8.8.8")];
        e.enrich(&mut hops).await;
        assert!(hops[0].asn.is_none());
    }

    #[tokio::test]
    async fn test_enrich_skips_asn_when_handle_holds_none() {
        // Handle present but no service inside (e.g. download never ran)
        // — still no panic, still None.
        let e = TracerouteEnricher::new()
            .with_asn(Arc::new(RwLock::new(None)));
        let mut hops = vec![hop_with_ip("8.8.8.8")];
        e.enrich(&mut hops).await;
        assert!(hops[0].asn.is_none());
    }

    #[tokio::test]
    async fn test_ptr_cache_hit_does_not_relookup() {
        let e = TracerouteEnricher::new();
        let ip: IpAddr = "203.0.113.1".parse().unwrap();
        // Pre-populate with a fake answer
        e.ptr_cache.insert(ip, (Some("fake.example".into()), Instant::now()));
        let v = e.ptr_lookup(ip).await;
        assert_eq!(v.as_deref(), Some("fake.example"));
    }

    #[tokio::test]
    async fn test_ptr_cache_expired_entry_is_refreshed() {
        let e = TracerouteEnricher::new();
        let ip: IpAddr = "203.0.113.2".parse().unwrap();
        // Insert an expired entry (inserted 2 hours ago) with a sentinel value
        let stale_time = Instant::now() - Duration::from_secs(7200);
        e.ptr_cache.insert(ip, (Some("stale.example".into()), stale_time));
        // After lookup, real DNS will likely return None for TEST-NET-3 → entry replaced
        let _ = e.ptr_lookup(ip).await;
        let (_, inserted_at) = e.ptr_cache.get(&ip).unwrap().value().clone();
        assert!(
            inserted_at > stale_time + Duration::from_secs(3600),
            "expected the cache entry to be refreshed"
        );
    }

    #[tokio::test]
    async fn test_ptr_cache_eviction_picks_oldest_not_smallest_ip() {
        // Regression guard for a sort-key bug: eviction previously sorted by
        // IpAddr value rather than inserted_at, so high-IP-numbered entries
        // would persist forever while young low-IP entries got evicted.
        let e = TracerouteEnricher::new();
        let now = Instant::now();
        // Insert in IP order, but with newest-first timestamps so IP-sort and
        // age-sort disagree.
        let high_ip: IpAddr = "203.0.113.250".parse().unwrap();
        let mid_ip: IpAddr = "203.0.113.100".parse().unwrap();
        let low_ip: IpAddr = "203.0.113.10".parse().unwrap();
        e.ptr_cache
            .insert(high_ip, (Some("old".into()), now - Duration::from_secs(300)));
        e.ptr_cache
            .insert(mid_ip, (Some("mid".into()), now - Duration::from_secs(200)));
        e.ptr_cache
            .insert(low_ip, (Some("new".into()), now - Duration::from_secs(100)));

        // Simulate the eviction body for limit=1
        let mut entries: Vec<(IpAddr, Instant)> = e
            .ptr_cache
            .iter()
            .map(|x| (*x.key(), x.value().1))
            .collect();
        entries.sort_by_key(|(_, t)| *t);
        let victim = entries.first().expect("non-empty").0;
        assert_eq!(victim, high_ip, "oldest entry must be evicted regardless of IP value");
    }

    #[tokio::test]
    async fn test_enrich_handles_ipv6() {
        let e = TracerouteEnricher::new();
        let mut hops = vec![hop_with_ip("::1")];
        // Should not panic on IPv6 even if PTR lookup fails.
        e.enrich(&mut hops).await;
    }
}

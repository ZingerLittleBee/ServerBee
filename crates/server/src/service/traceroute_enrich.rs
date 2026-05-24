use dashmap::DashMap;
use serverbee_common::types::TracerouteHop;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PTR_CACHE_TTL: Duration = Duration::from_secs(3600);
const PTR_CACHE_MAX: usize = 4096;

#[derive(Clone, Default)]
pub struct TracerouteEnricher {
    /// IpAddr -> (hostname_or_none, inserted_at). `None` means "we tried and got nothing"
    /// — we still cache the negative result to avoid hammering DNS.
    ptr_cache: Arc<DashMap<IpAddr, (Option<String>, Instant)>>,
}

impl TracerouteEnricher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fill in `hostname` for every hop with an IP. Best-effort; failures
    /// leave the field at whatever value the agent sent. ASN is deferred —
    /// existing GeoIP DB is country-only, IP quality data is per-agent, not
    /// per arbitrary hop IP.
    pub async fn enrich(&self, hops: &mut [TracerouteHop]) {
        // Walk hops, prefer `ips[0]` (new schema) over `ip` (legacy).
        for hop in hops.iter_mut() {
            if hop.hostname.is_some() {
                continue;
            }
            let ip_str = hop.ips.first().cloned().or_else(|| hop.ip.clone());
            let Some(s) = ip_str else { continue };
            let Ok(ip) = IpAddr::from_str(&s) else { continue };
            hop.hostname = self.ptr_lookup(ip).await;
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
            // Drop ~1/16 of entries by scanning and removing the oldest we see.
            let limit = PTR_CACHE_MAX / 16;
            let mut victims: Vec<IpAddr> = self
                .ptr_cache
                .iter()
                .map(|e| (*e.key(), e.value().1))
                .collect::<Vec<_>>()
                .into_iter()
                .take(limit)
                .map(|(k, _)| k)
                .collect();
            // Sort by inserted_at ASC and drop the oldest.
            victims.sort();
            for v in victims {
                self.ptr_cache.remove(&v);
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
    async fn test_enrich_leaves_asn_field_none() {
        // Regression guard: a future ASN feature must not silently drop the field.
        let e = TracerouteEnricher::new();
        let mut hops = vec![hop_with_ip("127.0.0.1")];
        e.enrich(&mut hops).await;
        assert!(hops[0].asn.is_none(), "ASN must remain None in this iteration");
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
    async fn test_enrich_handles_ipv6() {
        let e = TracerouteEnricher::new();
        let mut hops = vec![hop_with_ip("::1")];
        // Should not panic on IPv6 even if PTR lookup fails.
        e.enrich(&mut hops).await;
    }
}

use super::*;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;

pub(super) const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_REDIRECTS: usize = 10;

pub(super) struct SystemResolver;

impl Resolve for SystemResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addresses: Vec<_> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

pub(super) struct DownloadResolver {
    resolver: Arc<dyn Resolve>,
    initial_host: String,
    initial_addresses: Vec<SocketAddr>,
    allow_local: bool,
    target_hosts: Mutex<HashSet<String>>,
    timeout: Duration,
}

impl DownloadResolver {
    pub(super) async fn new(
        initial: &Url,
        resolver: Arc<dyn Resolve>,
        timeout: Duration,
    ) -> Result<Arc<Self>> {
        let host = initial.host_str().context("download URL has no host")?.to_owned();
        let addresses = match literal_ip(initial) {
            Some(ip) => vec![SocketAddr::new(ip, 0)],
            None => tokio::time::timeout(CONNECT_TIMEOUT, resolver.resolve(host.parse()?))
                .await
                .context("download DNS resolution timed out")?
                .map_err(anyhow::Error::from_boxed)?
                .collect(),
        };
        anyhow::ensure!(!addresses.is_empty(), "download DNS returned no addresses");
        let allow_local = addresses.iter().all(|address| is_local_address(address.ip()));
        if !allow_local
            && (is_local_hostname(&host)
                || addresses.iter().any(|address| is_local_address(address.ip())))
        {
            return Err(rejected_host(&host).into());
        }
        // Reuse the exact first lookup so DNS rebinding cannot turn a public
        // configured endpoint into a local one between classification and connect.
        Ok(Arc::new(Self {
            resolver,
            initial_host: normalized_host(&host),
            initial_addresses: addresses,
            allow_local,
            target_hosts: Mutex::new([normalized_host(&host)].into_iter().collect()),
            timeout,
        }))
    }

    pub(super) async fn check_url(&self, url: &Url) -> Result<()> {
        validate_http_scheme(url)?;
        let host = url.host_str().context("download URL has no host")?;
        self.target_hosts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(normalized_host(host));
        if let Some(ip) = literal_ip(url) {
            if !self.allow_local && is_local_address(ip) {
                return Err(rejected_host(host).into());
            }
        } else {
            if !self.allow_local && is_local_hostname(host) {
                return Err(rejected_host(host).into());
            }
            // This lookup runs even when a proxy would resolve the target itself.
            // SystemResolver uses tokio::net::lookup_host; tests inject DNS answers.
            let addresses: Vec<_> =
                tokio::time::timeout(CONNECT_TIMEOUT, self.resolver.resolve(host.parse()?))
                    .await
                    .context("download DNS resolution timed out")?
                    .map_err(anyhow::Error::from_boxed)?
                    .collect();
            anyhow::ensure!(!addresses.is_empty(), "download DNS returned no addresses");
            if !self.allow_local && addresses.iter().any(|address| is_local_address(address.ip())) {
                return Err(rejected_host(host).into());
            }
        }
        Ok(())
    }

    // All callers send empty GET/POST requests. Follow redirects here so the
    // asynchronous preflight happens before each hop, also through system proxies.
    pub(super) async fn request(
        &self,
        client: &reqwest::Client,
        mut method: reqwest::Method,
        mut url: Url,
    ) -> Result<Response> {
        let mut referer = None;
        let mut authorization: Option<reqwest::header::HeaderValue> = None;
        let deadline = tokio::time::Instant::now() + self.timeout;
        for redirects in 0..=MAX_REDIRECTS {
            tokio::time::timeout_at(deadline, self.check_url(&url))
                .await
                .context("download request timed out")??;
            let mut request = client
                .request(method.clone(), url.clone())
                .timeout(deadline.saturating_duration_since(tokio::time::Instant::now()));
            if let Some(value) = &referer {
                request = request.header(reqwest::header::REFERER, value);
            }
            let mut request = request.build().map_err(reqwest::Error::without_url)?;
            if !request.headers().contains_key(reqwest::header::AUTHORIZATION)
                && let Some(value) = &authorization
            {
                request.headers_mut().insert(reqwest::header::AUTHORIZATION, value.clone());
            }
            authorization = request.headers().get(reqwest::header::AUTHORIZATION).cloned();
            url = request.url().clone();
            let response = client.execute(request).await.map_err(reqwest::Error::without_url)?;
            let status = response.status().as_u16();
            if !matches!(status, 301 | 302 | 303 | 307 | 308) {
                return Ok(response);
            }
            let Some(location) = response.headers().get(reqwest::header::LOCATION) else {
                return Ok(response);
            };
            anyhow::ensure!(redirects < MAX_REDIRECTS, "too many download redirects");
            let next = url
                .join(location.to_str().context("invalid download redirect header")?)
                .context("invalid download redirect URL")?;
            if next.origin() != url.origin() {
                authorization = None;
            }
            if status == 303 && method != reqwest::Method::HEAD
                || matches!(status, 301 | 302) && method == reqwest::Method::POST
            {
                method = reqwest::Method::GET;
            }
            referer = if url.scheme() == "https" && next.scheme() == "http" {
                None
            } else {
                let mut previous = url.clone();
                let _ = previous.set_username("");
                let _ = previous.set_password(None);
                previous.set_fragment(None);
                Some(previous.to_string())
            };
            url = next;
            drop(response);
        }
        unreachable!("the last redirect returns an error")
    }
}

impl Resolve for DownloadResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        if normalized_host(&host) == self.initial_host {
            let addresses = self.initial_addresses.clone();
            return Box::pin(async move { Ok(Box::new(addresses.into_iter()) as Addrs) });
        }
        // reqwest also resolves the explicitly configured proxy endpoint through
        // this resolver. Only destination hosts checked by request() are subject
        // to the target policy; a trusted local proxy must remain usable.
        let check_target = self
            .target_hosts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(&normalized_host(&host));
        let allow_local = self.allow_local || !check_target;
        let resolution = self.resolver.resolve(name);
        Box::pin(async move {
            let addresses: Vec<_> = resolution.await?.collect();
            if !allow_local && addresses.iter().any(|address| is_local_address(address.ip())) {
                return Err(
                    Box::new(rejected_host(&host)) as Box<dyn std::error::Error + Send + Sync>
                );
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

fn rejected_host(host: &str) -> std::io::Error {
    tracing::warn!(host, "download refused a local destination");
    std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        format!("local download destination is blocked: {host}"),
    )
}

fn literal_ip(url: &Url) -> Option<IpAddr> {
    url.host_str()?.trim_start_matches('[').trim_end_matches(']').parse().ok()
}

fn normalized_host(host: &str) -> String {
    host.trim_end_matches('.').to_ascii_lowercase()
}

fn is_local_hostname(host: &str) -> bool {
    let host = normalized_host(host);
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "local"
        || host.ends_with(".local")
}

fn is_local_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            address.is_loopback()
                || address.is_private()
                || address.is_link_local()
                || address.is_unspecified()
                || address.is_broadcast()
                || address.is_multicast()
                || address.octets()[0] == 0
                || matches!(address.octets(), [100, 64..=127, _, _])
        }
        IpAddr::V6(address) => {
            address.is_loopback()
                || address.is_unspecified()
                || address.is_unique_local()
                || address.is_unicast_link_local()
                || address.is_multicast()
                || address
                    .to_ipv4_mapped()
                    .is_some_and(|address| is_local_address(IpAddr::V4(address)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_hostnames_include_subdomains_case_and_trailing_dots() {
        for host in [
            "localhost",
            "LOCALHOST.",
            "child.localhost",
            "child.localhost.",
            "node.local",
            "node.local.",
            "local",
        ] {
            assert!(is_local_hostname(host), "{host}");
        }
        for host in
            ["example.com", "example.com.", "localhost.example.com", "notlocalhost", "notlocal"]
        {
            assert!(!is_local_hostname(host), "{host}");
        }
    }

    #[tokio::test]
    async fn a_target_rebinding_after_preflight_is_rejected_by_the_connection_resolver() {
        use std::sync::atomic::AtomicUsize;
        struct RebindingResolver(AtomicUsize);
        impl Resolve for RebindingResolver {
            fn resolve(&self, name: Name) -> Resolving {
                let ip =
                    if name.as_str() == "cdn.test" && self.0.fetch_add(1, Ordering::Relaxed) > 0 {
                        "127.0.0.1:0"
                    } else {
                        "8.8.8.8:0"
                    };
                let address: SocketAddr = ip.parse().unwrap();
                Box::pin(async move { Ok(Box::new(std::iter::once(address)) as Addrs) })
            }
        }
        let guarded = DownloadResolver::new(
            &Url::parse("http://source.test").unwrap(),
            Arc::new(RebindingResolver(AtomicUsize::new(0))),
            Duration::from_secs(3),
        )
        .await
        .unwrap();
        guarded.check_url(&Url::parse("http://cdn.test/archive").unwrap()).await.unwrap();
        assert!(guarded.resolve("cdn.test".parse().unwrap()).await.is_err());
    }

    #[tokio::test]
    async fn resolver_rejects_local_answers_and_pins_the_initial_public_lookup() {
        use std::sync::atomic::AtomicUsize;
        struct RebindingResolver(AtomicUsize);
        impl Resolve for RebindingResolver {
            fn resolve(&self, _: Name) -> Resolving {
                let address = if self.0.fetch_add(1, Ordering::Relaxed) == 0 {
                    "8.8.8.8:0"
                } else {
                    "127.0.0.1:0"
                };
                let address: SocketAddr = address.parse().unwrap();
                Box::pin(async move { Ok(Box::new(std::iter::once(address)) as Addrs) })
            }
        }
        let resolver = Arc::new(RebindingResolver(AtomicUsize::new(0)));
        let guarded = DownloadResolver::new(
            &Url::parse("http://source.test/api").unwrap(),
            resolver.clone(),
            Duration::from_secs(3),
        )
        .await
        .unwrap();
        for _ in 0..2 {
            let addresses: Vec<_> =
                guarded.resolve("source.test".parse().unwrap()).await.unwrap().collect();
            assert_eq!(addresses, ["8.8.8.8:0".parse::<SocketAddr>().unwrap()]);
        }
        assert_eq!(resolver.0.load(Ordering::Relaxed), 1);
        assert!(guarded.check_url(&Url::parse("http://local.test").unwrap()).await.is_err());
        assert!(guarded.resolve("local.test".parse().unwrap()).await.is_err());
        for value in
            ["http://2130706433", "http://0x7f000001", "http://127.1", "http://[::ffff:127.0.0.1]"]
        {
            assert!(guarded.check_url(&Url::parse(value).unwrap()).await.is_err(), "{value}");
        }
    }

    #[test]
    fn local_addresses_cover_ipv4_ipv6_and_mapped_forms() {
        for address in [
            "127.0.0.1",
            "127.255.255.255",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.1.1",
            "100.64.0.1",
            "100.127.255.255",
            "0.0.0.0",
            "0.1.2.3",
            "255.255.255.255",
            "224.0.0.1",
            "::",
            "::1",
            "fc00::1",
            "fdff::1",
            "fe80::1",
            "febf::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:192.168.1.1",
            "::ffff:100.64.0.1",
        ] {
            assert!(is_local_address(address.parse().unwrap()), "{address}");
        }
        for address in [
            "8.8.8.8",
            "1.1.1.1",
            "100.63.255.255",
            "100.128.0.1",
            "172.15.255.255",
            "172.32.0.1",
            "2001:4860:4860::8888",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(!is_local_address(address.parse().unwrap()), "{address}");
        }
    }
}

pub struct DefaultPeer {
    pub label: &'static str,
    pub node_id: &'static str,
}

pub const DEFAULT_PEERS: [DefaultPeer; 0] = [];

pub const ENV_OVERRIDE: &str = "KAI_DEFAULT_PEERS";

pub fn seeds() -> Vec<(String, Vec<String>)> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(value) = std::env::var(ENV_OVERRIDE) {
        return value
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty() && *entry != "none")
            .enumerate()
            .map(|(index, entry)| (format!("env peer {}", index + 1), vec![entry.to_string()]))
            .collect();
    }
    DEFAULT_PEERS
        .iter()
        .map(|peer| (peer.label.to_string(), vec![peer.node_id.to_string()]))
        .collect()
}

pub fn label_of(node_id: &str) -> Option<&'static str> {
    DEFAULT_PEERS
        .iter()
        .find(|peer| peer.node_id == node_id)
        .map(|peer| peer.label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_builds_have_no_baked_development_peers() {
        assert!(DEFAULT_PEERS.is_empty());
        assert_eq!(label_of("https://example.com"), None);
    }

    #[test]
    fn a_default_peer_is_seeded_by_its_baked_node_id_alone() {
        let seeds = seeds();
        assert_eq!(seeds.len(), DEFAULT_PEERS.len());
        for (index, (label, forms)) in seeds.iter().enumerate() {
            assert_eq!(label, DEFAULT_PEERS[index].label);
            assert_eq!(forms, &vec![DEFAULT_PEERS[index].node_id.to_string()]);
            assert!(
                !forms.iter().any(|form| form.starts_with("http")),
                "no gateway url is baked into the client"
            );
        }
    }
}

pub fn proxied_origin(origin: &str) -> Option<String> {
    let rest = origin.strip_prefix("https://")?;
    let host = rest.split(':').next().unwrap_or(rest);
    if host == "localhost" || host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    Some(origin.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod origin_tests {
    use super::proxied_origin;

    #[test]
    fn only_a_named_https_origin_is_tried_as_a_gateway_proxy() {
        assert_eq!(
            proxied_origin("https://example.com"),
            Some("https://example.com".into())
        );
        assert_eq!(proxied_origin("http://127.0.0.1:8123"), None);
        assert_eq!(proxied_origin("https://127.0.0.1:8443"), None);
        assert_eq!(proxied_origin("https://localhost:8443"), None);
        assert_eq!(proxied_origin("http://example.com"), None);
    }
}

//! The server's self-signed certificate, made once and kept, and the Mac's addresses on the local network.

use anyhow::Result;
use kara_core::store::write_private;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const DAY: i64 = 86_400;
const VALID_DAYS: i64 = 800;
const RENEW_DAYS: i64 = 30;

#[derive(Serialize, Deserialize)]
struct Kept {
    name: String,
    ips: Vec<IpAddr>,
    made_at: i64,
    cert: String,
    key: String,
}

/// PEM certificate and key for `name` and `ips` at unix time `now`: the kept pair while it covers them and has a month left,
/// otherwise a new pair covering them and every address the kept one did, saved in `dir`.
pub fn ensure(dir: &Path, name: &str, ips: &[IpAddr], now: i64) -> Result<(Vec<u8>, Vec<u8>)> {
    let file = dir.join("certificate.json");
    let kept = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Kept>(&b).ok());
    if let Some(k) = &kept {
        if k.name == name && ips.iter().all(|ip| k.ips.contains(ip)) && now < k.made_at + (VALID_DAYS - RENEW_DAYS) * DAY {
            return Ok((k.cert.clone().into_bytes(), k.key.clone().into_bytes()));
        }
    }
    let mut all = kept.map(|k| k.ips).unwrap_or_default();
    for ip in ips {
        if !all.contains(ip) {
            all.push(*ip);
        }
    }
    let mut params = rcgen::CertificateParams::new(vec![name.to_string()])?;
    params.subject_alt_names.extend(all.iter().map(|ip| rcgen::SanType::IpAddress(*ip)));
    params.distinguished_name.push(rcgen::DnType::CommonName, "KaraAlwaysOK");
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    params.not_before = time::OffsetDateTime::from_unix_timestamp(now - DAY)?;
    params.not_after = time::OffsetDateTime::from_unix_timestamp(now + VALID_DAYS * DAY)?;
    let key = rcgen::KeyPair::generate()?;
    let cert = params.self_signed(&key)?;
    let k = Kept { name: name.to_string(), ips: all, made_at: now, cert: cert.pem(), key: key.serialize_pem() };
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    write_private(&file, &serde_json::to_vec(&k)?)?;
    Ok((k.cert.into_bytes(), k.key.into_bytes()))
}

/// The Mac's private IPv4 addresses, Wi-Fi and Ethernet first.
pub fn lan_ips() -> Vec<IpAddr> {
    let mut found: Vec<(bool, String, IpAddr)> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i.ip().is_ipv4() && !i.is_loopback() && is_lan(i.ip()))
        .map(|i| (!i.name.starts_with("en"), i.name.clone(), i.ip()))
        .collect();
    found.sort();
    found.into_iter().map(|(.., ip)| ip).collect()
}

/// Whether an address is on the local network: private, link-local or this Mac.
pub fn is_lan(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || v4.is_loopback(),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_lan(v4.into()),
            None => v6.is_loopback() || v6.segments()[0] & 0xfe00 == 0xfc00 || v6.segments()[0] & 0xffc0 == 0xfe80,
        },
    }
}

/// This Mac's name on the local network, like "Yasushis-MacBook-Pro.local".
pub fn local_name() -> Option<String> {
    let out = std::process::Command::new("scutil").args(["--get", "LocalHostName"]).output().ok()?;
    let name = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !name.is_empty()).then(|| format!("{name}.local"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_certificate_is_reused_until_the_mac_gets_a_new_address_or_it_nears_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let home: IpAddr = "192.168.1.20".parse().unwrap();
        let office: IpAddr = "10.0.4.7".parse().unwrap();
        let now = 1_790_000_000;
        let first = ensure(dir.path(), "mac.local", &[home], now).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!((mode(dir.path()), mode(&dir.path().join("certificate.json"))), (0o700, 0o600), "only this user reads the key");
        assert_eq!(ensure(dir.path(), "mac.local", &[home], now + 30 * DAY).unwrap(), first);
        let moved = ensure(dir.path(), "mac.local", &[office], now + 31 * DAY).unwrap();
        assert_ne!(moved, first);
        assert_eq!(ensure(dir.path(), "mac.local", &[home], now + 32 * DAY).unwrap(), moved, "still covers the first address");
        assert_ne!(ensure(dir.path(), "mac.local", &[home], now + (31 + 771) * DAY).unwrap(), moved, "remade a month before it runs out");
    }

    #[test]
    fn only_local_network_addresses_count_as_lan() {
        let lan = ["192.168.1.9", "10.1.2.3", "172.16.0.5", "169.254.10.1", "127.0.0.1", "::1", "fe80::1", "fd12::3", "::ffff:192.168.0.2"];
        let outside = ["8.8.8.8", "172.32.0.1", "2001:db8::1", "::ffff:1.1.1.1"];
        for a in lan {
            assert!(is_lan(a.parse().unwrap()), "{a} is on the local network");
        }
        for a in outside {
            assert!(!is_lan(a.parse().unwrap()), "{a} is not");
        }
    }
}

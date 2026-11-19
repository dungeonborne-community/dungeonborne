//! HTTP client and URL policy: https everywhere; plain http only to loopback (mock server, local backend).
//! Certificates: the usual CA chain, or a leaf pinned by sha256 (built-in server without a domain, relay pin).

use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Url};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};

use crate::error::{bail, Result};

pub const USER_AGENT: &str = concat!("dbl/", env!("CARGO_PKG_VERSION"));

pub fn is_loopback_host(url: &Url) -> bool {
    matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
}

pub fn url_allowed(url: &Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => is_loopback_host(url),
        _ => false,
    }
}

pub fn check_url(s: &str) -> Result<Url> {
    let url = Url::parse(s.trim()).map_err(|e| crate::error::Error::new("bad_url", format!("{s}: {e}")))?;
    if !url_allowed(&url) {
        bail!("insecure_url", "{url}");
    }
    Ok(url)
}

/// Accepts a leaf whose sha256 (of the DER) is in `pins`, otherwise the CA chain if `webpki` is set.
/// Handshake signatures are always checked, so the peer must hold the certificate's private key.
#[derive(Debug)]
pub struct Verifier {
    pins: Vec<String>,
    webpki: Option<Arc<WebPkiServerVerifier>>,
    provider: Arc<CryptoProvider>,
}

impl Verifier {
    /// `pins` + CA chain (`fallback`), or exactly `pins` (`!fallback`)
    pub fn new(pins: Vec<String>, fallback: bool) -> Arc<Self> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let webpki = fallback.then(|| {
            let mut roots = RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone()).build().expect("webpki roots")
        });
        Arc::new(Self { pins: pins.into_iter().map(|p| p.to_ascii_lowercase()).collect(), webpki, provider })
    }

    /// Built-in pins (`builtin::CERT_SHA256`) + CA chain
    pub fn builtin() -> Arc<Self> {
        Self::new(crate::builtin::CERT_SHA256.iter().map(|s| s.to_string()).collect(), true)
    }
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        if self.pins.contains(&sha256_hex(end_entity.as_ref())) {
            return Ok(ServerCertVerified::assertion());
        }
        match &self.webpki {
            Some(w) => w.verify_server_cert(end_entity, intermediates, server_name, ocsp, now),
            None => Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure)),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

pub fn tls_config(verifier: Arc<Verifier>, tls13_only: bool) -> ClientConfig {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let versions: &[&rustls::SupportedProtocolVersion] =
        if tls13_only { &[&rustls::version::TLS13] } else { &[&rustls::version::TLS13, &rustls::version::TLS12] };
    ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(versions)
        .expect("tls versions supported")
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth()
}

pub fn client() -> Client {
    Client::builder()
        .use_preconfigured_tls(tls_config(Verifier::builtin(), false))
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::custom(|a| {
            if a.previous().len() >= 5 {
                a.error("too many redirects")
            } else if url_allowed(a.url()) {
                a.follow()
            } else {
                a.error("redirect to insecure url")
            }
        }))
        .build()
        .expect("http client")
}

/// Big downloads (mod packages, launcher updates): streamed to `<path>.part` and renamed at the end, sha256 on the fly.
/// No overall deadline (slow links are fine), but a stalled transfer fails after `STALL`.
/// `progress(done, total)` is called per chunk. Returns (sha256, bytes).
pub async fn download_to(
    client: &Client,
    url: &Url,
    path: &std::path::Path,
    limit: u64,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<(String, u64)> {
    use sha2::Digest;
    use tokio::io::AsyncWriteExt;
    const STALL: Duration = Duration::from_secs(30);
    let mut resp = client.get(url.clone()).timeout(Duration::from_secs(6 * 3600)).send().await?;
    if !resp.status().is_success() {
        bail!("http_status", "{url}: {}", resp.status());
    }
    let total = resp.content_length();
    if total.is_some_and(|t| t > limit) {
        bail!("too_large", "{url}: more than {limit} bytes");
    }
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let part = path.with_extension("part");
    let mut f = tokio::fs::File::create(&part).await?;
    let mut hash = sha2::Sha256::new();
    let mut done = 0u64;
    progress(0, total);
    loop {
        let chunk = match tokio::time::timeout(STALL, resp.chunk()).await {
            Ok(c) => c?,
            Err(_) => bail!("network", "{url}: no data for {}s", STALL.as_secs()),
        };
        let Some(chunk) = chunk else { break };
        done += chunk.len() as u64;
        if done > limit {
            drop(f);
            let _ = tokio::fs::remove_file(&part).await;
            bail!("too_large", "{url}: more than {limit} bytes");
        }
        hash.update(&chunk);
        f.write_all(&chunk).await?;
        progress(done, total);
    }
    f.flush().await?;
    drop(f);
    tokio::fs::rename(&part, path).await?;
    Ok((hex::encode(hash.finalize()), done))
}

/// GET with a hard size cap (manifests, CA).
pub async fn get_bytes(client: &Client, url: &Url, limit: usize) -> Result<Vec<u8>> {
    let mut resp = client.get(url.clone()).send().await?;
    if !resp.status().is_success() {
        bail!("http_status", "{url}: {}", resp.status());
    }
    let mut out = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if out.len() + chunk.len() > limit {
            bail!("too_large", "{url}: more than {limit} bytes");
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(data))
}

pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    /// Pinned built-in certificate: `DBL_PIN_TEST_URL=https://127.0.0.1:8443/ cargo test pinned_live -- --ignored`
    /// against any https server holding `deploy/tls/backend.crt` (name mismatch is fine: the leaf is pinned).
    #[tokio::test]
    #[ignore]
    async fn pinned_live() {
        let url = std::env::var("DBL_PIN_TEST_URL").expect("DBL_PIN_TEST_URL");
        let r = super::client().get(&url).send().await.expect("pinned certificate accepted");
        assert!(r.status().is_success());
        // a client without the pin must refuse the same server
        let plain = reqwest::Client::builder().build().unwrap();
        assert!(plain.get(&url).send().await.is_err(), "self-signed refused without the pin");
    }
}

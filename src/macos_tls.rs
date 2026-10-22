//! TLS to the tracker on macOS when a CA is configured: a pinned rustls verifier, in place of
//! native-tls. The agent is built here only on macOS; every other platform keeps native-tls in
//! `gitlab.rs`. Territory: `docs/map/reading-the-server.md`.

use crate::tracker::Error;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned};
use ureq::Agent;
use ureq::config::Config;
use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, Either, LazyBuffers, NextTimeout, TcpConnector,
    Transport, TransportAdapter,
};
use x509_parser::certificate::X509Certificate;
use x509_parser::extensions::GeneralName;

/// An agent that reaches the tracker over TLS verified against the configured CA alone, through
/// the pinned verifier rather than native-tls. `pem` is the configured CA file's bytes, `path`
/// only for error messages.
pub fn agent(config: Config, pem: &[u8], path: &Path) -> Result<Agent, Error> {
    let cas = parse_ca_ders(pem, path)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let algs = provider.signature_verification_algorithms;
    let client = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Source(format!("rustls: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedVerifier { cas, algs }))
        .with_no_client_auth();
    let connector = TcpConnector::default().chain(PinnedConnector {
        config: Arc::new(client),
    });
    Ok(Agent::with_parts(
        config,
        connector,
        DefaultResolver::default(),
    ))
}

/// The DER of each certificate in a PEM file; an error when the file holds none, so a misconfigured
/// CA fails here rather than at the first handshake.
fn parse_ca_ders(pem: &[u8], path: &Path) -> Result<Vec<CertificateDer<'static>>, Error> {
    let mut ders = Vec::new();
    for item in x509_parser::pem::Pem::iter_from_buffer(pem) {
        let item =
            item.map_err(|e| Error::Source(format!("CA certificate {}: {e}", path.display())))?;
        ders.push(CertificateDer::from(item.contents));
    }
    if ders.is_empty() {
        return Err(Error::Source(format!(
            "no PEM certificate in {}",
            path.display()
        )));
    }
    Ok(ders)
}

/// Trusts the configured CA and nothing else. It accepts a self-signed `CA:TRUE` sheet presented as
/// the server's own certificate — which rustls/webpki refuse as `CaUsedAsEndEntity` and native-tls
/// on macOS refuses for its validity span — by pinning: the presented leaf must be byte-identical
/// to a configured CA. Name, the certificate's own validity window, and the handshake signatures
/// are all still checked; only Apple's maximum-span policy is not.
#[derive(Debug)]
struct PinnedVerifier {
    cas: Vec<CertificateDer<'static>>,
    algs: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        _ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if !self.cas.iter().any(|ca| ca.as_ref() == end_entity.as_ref()) {
            return Err(rustls::Error::General(
                "the server certificate is not the configured CA (untrusted root)".into(),
            ));
        }
        check_name_and_validity(end_entity, server_name, now)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs.supported_schemes()
    }
}

/// The server name against the certificate's names, and `now` against its own validity window. The
/// span between those dates is never measured — that is the one check native-tls on macOS adds and
/// the one this verifier drops.
fn check_name_and_validity(
    cert: &CertificateDer<'_>,
    server_name: &ServerName<'_>,
    now: UnixTime,
) -> Result<(), rustls::Error> {
    let (_, parsed) = x509_parser::parse_x509_certificate(cert.as_ref())
        .map_err(|_| rustls::Error::General("could not parse the server certificate".into()))?;
    let now = now.as_secs() as i64;
    let validity = parsed.validity();
    if now < validity.not_before.timestamp() {
        return Err(rustls::Error::General(
            "the server certificate is not yet valid".into(),
        ));
    }
    if now > validity.not_after.timestamp() {
        return Err(rustls::Error::General(
            "the server certificate has expired".into(),
        ));
    }
    let host = match server_name {
        ServerName::DnsName(d) => d.as_ref().to_string(),
        other => {
            return Err(rustls::Error::General(format!(
                "unsupported server name {other:?}"
            )));
        }
    };
    if host_matches(&parsed, &host) {
        Ok(())
    } else {
        Err(rustls::Error::General(format!(
            "the server certificate is not valid for {host}"
        )))
    }
}

/// `host` against the certificate's SAN dNSNames, falling back to the subject CN only when there is
/// no SAN dNSName at all. A SAN that lists names but matches none is a refusal, not a CN fallback.
fn host_matches(cert: &X509Certificate, host: &str) -> bool {
    if let Ok(Some(san)) = cert.subject_alternative_name() {
        let mut had_dns = false;
        for name in &san.value.general_names {
            if let GeneralName::DNSName(dns) = name {
                had_dns = true;
                if dns_matches(dns, host) {
                    return true;
                }
            }
        }
        if had_dns {
            return false;
        }
    }
    cert.subject()
        .iter_common_name()
        .any(|cn| cn.as_str().map(|s| dns_matches(s, host)).unwrap_or(false))
}

/// One certificate name against a host: case-insensitive, with a single leading `*.` wildcard that
/// matches exactly one label.
fn dns_matches(pattern: &str, host: &str) -> bool {
    let pattern = pattern.trim_end_matches('.');
    let host = host.trim_end_matches('.');
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return match host.split_once('.') {
            Some((_, rest)) => rest.eq_ignore_ascii_case(suffix),
            None => false,
        };
    }
    pattern.eq_ignore_ascii_case(host)
}

/// Wraps the chained TCP transport in a TLS session that uses the pinned `ClientConfig`. This is
/// ureq's own rustls connector in miniature, kept because ureq exposes no way to hand its connector
/// a custom verifier.
#[derive(Debug)]
struct PinnedConnector {
    config: Arc<ClientConfig>,
}

impl<In: Transport> Connector<In> for PinnedConnector {
    type Out = Either<In, PinnedTlsTransport>;

    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        let Some(transport) = chained else {
            return Ok(None);
        };
        if !details.needs_tls() || transport.is_tls() {
            return Ok(Some(Either::A(transport)));
        }
        let host = details
            .uri
            .host()
            .ok_or(ureq::Error::Tls("no host in the uri for tls"))?;
        let name = ServerName::try_from(host)
            .map_err(|_| ureq::Error::Tls("invalid dns name for tls"))?
            .to_owned();
        let mut conn = ClientConnection::new(self.config.clone(), name)
            .map_err(|_| ureq::Error::Tls("could not start the tls connection"))?;
        let mut sock = TransportAdapter::new(transport.boxed());
        sock.set_timeout(details.timeout);
        conn.complete_io(&mut sock)?;
        let stream = StreamOwned { conn, sock };
        let buffers = LazyBuffers::new(
            details.config.input_buffer_size(),
            details.config.output_buffer_size(),
        );
        Ok(Some(Either::B(PinnedTlsTransport { buffers, stream })))
    }
}

struct PinnedTlsTransport {
    buffers: LazyBuffers,
    stream: StreamOwned<ClientConnection, TransportAdapter<Box<dyn Transport>>>,
}

impl std::fmt::Debug for PinnedTlsTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinnedTlsTransport").finish()
    }
}

impl Transport for PinnedTlsTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let output = &self.buffers.output()[..amount];
        self.stream.write_all(output)?;
        Ok(())
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let input = self.buffers.input_append_buf();
        let amount = self.stream.read(input)?;
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }

    fn is_open(&mut self) -> bool {
        self.stream.get_mut().get_mut().is_open()
    }

    fn is_tls(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const HOST: &str = "tracker.example";

    /// A self-signed `CA:TRUE` sheet (rcgen's default) for `names`, with the given validity window.
    fn cert(
        names: &[&str],
        not_before: (i32, u8, u8),
        not_after: (i32, u8, u8),
    ) -> CertificateDer<'static> {
        let mut params =
            rcgen::CertificateParams::new(names.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                .unwrap();
        params.not_before = rcgen::date_time_ymd(not_before.0, not_before.1, not_before.2);
        params.not_after = rcgen::date_time_ymd(not_after.0, not_after.1, not_after.2);
        let key = rcgen::KeyPair::generate().unwrap();
        params.self_signed(&key).unwrap().der().clone()
    }

    /// A ten-year sheet for HOST, valid across `now`.
    fn ten_year() -> CertificateDer<'static> {
        cert(&[HOST], (2025, 7, 18), (2035, 7, 16))
    }

    fn now() -> UnixTime {
        // 2026-10-22, inside the ten-year window.
        UnixTime::since_unix_epoch(Duration::from_secs(1_792_800_000))
    }

    fn name() -> ServerName<'static> {
        ServerName::try_from(HOST).unwrap()
    }

    fn verify(
        cas: Vec<CertificateDer<'static>>,
        leaf: &CertificateDer<'_>,
    ) -> Result<(), rustls::Error> {
        let v = PinnedVerifier {
            cas,
            algs: rustls::crypto::ring::default_provider().signature_verification_algorithms,
        };
        v.verify_server_cert(leaf, &[], &name(), &[], now())
            .map(|_| ())
    }

    #[test]
    fn a_pinned_ten_year_self_signed_sheet_is_accepted() {
        // The measured case: Security.framework refused this for its span; the pinned verifier does not.
        let c = ten_year();
        assert!(verify(vec![c.clone()], &c).is_ok());
    }

    #[test]
    fn a_certificate_not_among_the_configured_cas_is_refused() {
        let configured = ten_year();
        let other = cert(&[HOST], (2025, 7, 18), (2035, 7, 16)); // same name, different key → different DER
        let e = verify(vec![configured], &other).unwrap_err().to_string();
        assert!(e.contains("untrusted root"), "{e}");
    }

    #[test]
    fn an_expired_certificate_is_refused_even_when_pinned() {
        let c = cert(&[HOST], (1999, 1, 1), (2000, 1, 1));
        let e = verify(vec![c.clone()], &c).unwrap_err().to_string();
        assert!(e.contains("expired"), "{e}");
    }

    #[test]
    fn a_not_yet_valid_certificate_is_refused_even_when_pinned() {
        let c = cert(&[HOST], (2035, 1, 1), (2036, 1, 1));
        let e = verify(vec![c.clone()], &c).unwrap_err().to_string();
        assert!(e.contains("not yet valid"), "{e}");
    }

    #[test]
    fn a_pinned_certificate_for_a_different_host_is_refused() {
        let c = cert(&["other.example"], (2025, 7, 18), (2035, 7, 16));
        let e = verify(vec![c.clone()], &c).unwrap_err().to_string();
        assert!(e.contains("not valid for"), "{e}");
    }

    #[test]
    fn a_ca_file_holding_more_than_one_certificate_pins_to_any_of_them() {
        let a = cert(&["a.example"], (2025, 7, 18), (2035, 7, 16));
        let b = ten_year();
        assert!(verify(vec![a, b.clone()], &b).is_ok());
    }

    #[test]
    fn parse_ca_ders_reads_every_pem_and_rejects_an_empty_file() {
        let mut p = rcgen::CertificateParams::new(vec![HOST.to_string()]).unwrap();
        p.not_before = rcgen::date_time_ymd(2025, 7, 18);
        p.not_after = rcgen::date_time_ymd(2035, 7, 16);
        let one = p
            .self_signed(&rcgen::KeyPair::generate().unwrap())
            .unwrap()
            .pem();
        let two = p
            .self_signed(&rcgen::KeyPair::generate().unwrap())
            .unwrap()
            .pem();
        let pem = format!("{one}{two}");
        let path = Path::new("ca.pem");
        assert_eq!(parse_ca_ders(pem.as_bytes(), path).unwrap().len(), 2);
        assert!(parse_ca_ders(b"not a certificate", path).is_err());
    }

    #[test]
    fn dns_matches_is_case_insensitive_and_wildcards_one_label() {
        assert!(dns_matches("Tracker.Example", "tracker.example"));
        assert!(dns_matches("*.example", "tracker.example"));
        assert!(!dns_matches("*.example", "a.b.example"));
        assert!(!dns_matches("*.example", "example"));
        assert!(!dns_matches("tracker.example", "other.example"));
    }
}

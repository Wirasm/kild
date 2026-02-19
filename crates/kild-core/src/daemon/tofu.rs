//! TOFU (Trust On First Use) fingerprint verifier for TLS connections.
//!
//! The client pins the daemon's TLS certificate fingerprint on first use
//! and rejects any subsequent connection where the cert fingerprint doesn't
//! match. This provides MITM protection without a CA.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};
use sha2::{Digest, Sha256};

/// Compute the SHA-256 fingerprint of a DER-encoded certificate.
pub fn cert_fingerprint(cert: &CertificateDer<'_>) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(cert.as_ref());
    h.finalize().into()
}

/// Format a certificate fingerprint as "sha256:<hex>".
pub fn fingerprint_hex(cert: &CertificateDer<'_>) -> String {
    format!("sha256:{}", hex::encode(cert_fingerprint(cert)))
}

/// Parse a fingerprint from "sha256:<hex>" or bare 64-char hex string.
///
/// Returns the raw 32-byte fingerprint on success, or an error message on failure.
pub fn parse_fingerprint(s: &str) -> Result<[u8; 32], String> {
    let hex_str = s.strip_prefix("sha256:").unwrap_or(s);
    let bytes = hex::decode(hex_str).map_err(|e| format!("invalid fingerprint hex: {}", e))?;
    bytes
        .try_into()
        .map_err(|_| "fingerprint must be 32 bytes (64 hex chars)".to_string())
}

/// A `ServerCertVerifier` that accepts connections only if the server's
/// certificate SHA-256 fingerprint matches the pinned value.
///
/// This is not a CA-based verifier. It does not validate expiry, hostname, or
/// the certificate chain. It only checks: "is this the exact cert we expect?"
///
/// Security model: the pinned fingerprint is a shared secret between the user
/// and their daemon. An attacker without the fingerprint cannot complete a
/// useful connection.
#[derive(Debug)]
pub struct TofuVerifier {
    pinned: [u8; 32],
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl TofuVerifier {
    /// Create a new `TofuVerifier` pinning the given 32-byte fingerprint.
    pub fn new(pinned: [u8; 32]) -> Arc<Self> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        Arc::new(Self { pinned, provider })
    }
}

impl ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let got = cert_fingerprint(end_entity);
        if got == self.pinned {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(TlsError::General(format!(
                "TLS cert fingerprint mismatch — expected {} got {}. \
                 If the daemon regenerated its certificate, obtain the new fingerprint with: \
                 openssl x509 -in ~/.kild/certs/daemon.crt -fingerprint -sha256 -noout",
                hex::encode(self.pinned),
                hex::encode(got)
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fingerprint_with_prefix() {
        let hex = "a".repeat(64);
        let fp = format!("sha256:{}", hex);
        let result = parse_fingerprint(&fp);
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        assert_eq!(result.unwrap(), [0xaa; 32]);
    }

    #[test]
    fn test_parse_fingerprint_bare_hex() {
        let hex = "bb".repeat(32);
        let result = parse_fingerprint(&hex);
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        assert_eq!(result.unwrap(), [0xbb; 32]);
    }

    #[test]
    fn test_parse_fingerprint_wrong_length() {
        let result = parse_fingerprint("sha256:aabb");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("32 bytes"));
    }

    #[test]
    fn test_parse_fingerprint_invalid_hex() {
        let result = parse_fingerprint("sha256:zzzz");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid fingerprint hex"));
    }

    #[test]
    fn test_cert_fingerprint_deterministic() {
        // Fake cert DER bytes
        let cert_der = CertificateDer::from(vec![1u8, 2, 3, 4, 5]);
        let fp1 = cert_fingerprint(&cert_der);
        let fp2 = cert_fingerprint(&cert_der);
        assert_eq!(fp1, fp2, "fingerprint must be deterministic");
    }

    #[test]
    fn test_fingerprint_hex_format() {
        let cert_der = CertificateDer::from(vec![1u8, 2, 3]);
        let hex = fingerprint_hex(&cert_der);
        assert!(hex.starts_with("sha256:"), "must start with sha256:");
        let rest = hex.strip_prefix("sha256:").unwrap();
        assert_eq!(rest.len(), 64, "hex part must be 64 chars");
    }
}

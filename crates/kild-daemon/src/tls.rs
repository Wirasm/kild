//! TLS certificate loading and server config construction.
//!
//! On first daemon start with `bind_tcp` configured, generates a self-signed
//! certificate via `rcgen` and saves it to `~/.kild/certs/`. Subsequent starts
//! load the existing PEM files.

use rcgen::{CertifiedKey, generate_simple_self_signed};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

use crate::errors::DaemonError;

/// Load an existing PEM cert+key pair, or generate a self-signed one.
///
/// If both files exist, reads and returns their contents.
/// If either is missing, generates a new self-signed cert for "kild-daemon",
/// writes both PEM files to disk, and returns the DER-encoded forms.
///
/// The generated cert is self-signed. Clients authenticate via fingerprint
/// pinning (TOFU), not via a CA. Hostname validation is intentionally bypassed
/// by `TofuVerifier`, so the SAN is informational only.
///
/// The key file is written with `0o600` permissions (owner read/write only).
pub fn load_or_generate_cert(
    cert_path: &Path,
    key_path: &Path,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), DaemonError> {
    if cert_path.exists() && key_path.exists() {
        let certs = rustls_pemfile::certs(&mut BufReader::new(
            File::open(cert_path).map_err(DaemonError::Io)?,
        ))
        .collect::<Result<Vec<_>, _>>()
        .map_err(DaemonError::Io)?;

        let key = rustls_pemfile::private_key(&mut BufReader::new(
            File::open(key_path).map_err(DaemonError::Io)?,
        ))
        .map_err(DaemonError::Io)?
        .ok_or_else(|| DaemonError::TlsConfig("no private key found in key file".into()))?;

        return Ok((certs, key));
    }

    info!(
        event = "daemon.tls.cert_generating",
        cert_path = %cert_path.display()
    );

    let CertifiedKey { cert, signing_key } =
        generate_simple_self_signed(vec!["kild-daemon".to_string()])
            .map_err(|e| DaemonError::TlsConfig(e.to_string()))?;

    if let Some(parent) = cert_path.parent() {
        fs::create_dir_all(parent).map_err(DaemonError::Io)?;
    }
    fs::write(cert_path, cert.pem()).map_err(DaemonError::Io)?;
    // Write key with 0o600 so only the daemon process owner can read it.
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(key_path)
        .and_then(|mut f| f.write_all(signing_key.serialize_pem().as_bytes()))
        .map_err(DaemonError::Io)?;

    info!(
        event = "daemon.tls.cert_generated",
        cert_path = %cert_path.display()
    );

    let cert_der = cert.der().clone();
    let key_der = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(signing_key.serialize_der()));
    Ok((vec![cert_der], key_der))
}

/// Build a `rustls::ServerConfig` from the given cert and key.
///
/// Uses `builder_with_provider` so the crypto provider is explicit and
/// independent of whether `install_default()` has been called.
pub fn build_server_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<rustls::ServerConfig>, DaemonError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| DaemonError::TlsConfig(e.to_string()))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| DaemonError::TlsConfig(e.to_string()))?;
    Ok(Arc::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_or_generate_creates_cert_when_missing() {
        let dir = tempdir().unwrap();
        let cert_path = dir.path().join("daemon.crt");
        let key_path = dir.path().join("daemon.key");

        let result = load_or_generate_cert(&cert_path, &key_path);
        assert!(result.is_ok(), "should generate cert: {:?}", result.err());

        let (certs, _key) = result.unwrap();
        assert!(!certs.is_empty(), "should return at least one cert");
        assert!(cert_path.exists(), "cert file should be written");
        assert!(key_path.exists(), "key file should be written");
    }

    #[test]
    fn test_load_or_generate_loads_existing_cert() {
        let dir = tempdir().unwrap();
        let cert_path = dir.path().join("daemon.crt");
        let key_path = dir.path().join("daemon.key");

        // First call generates
        let _ = load_or_generate_cert(&cert_path, &key_path).unwrap();
        let cert_mtime = cert_path.metadata().unwrap().modified().unwrap();

        // Second call loads (not regenerated)
        let result = load_or_generate_cert(&cert_path, &key_path);
        assert!(
            result.is_ok(),
            "should load existing cert: {:?}",
            result.err()
        );

        let cert_mtime2 = cert_path.metadata().unwrap().modified().unwrap();
        assert_eq!(
            cert_mtime, cert_mtime2,
            "cert file should not be overwritten"
        );
    }

    #[test]
    fn test_build_server_config_succeeds() {
        let dir = tempdir().unwrap();
        let cert_path = dir.path().join("daemon.crt");
        let key_path = dir.path().join("daemon.key");

        let (certs, key) = load_or_generate_cert(&cert_path, &key_path).unwrap();
        let result = build_server_config(certs, key);
        assert!(
            result.is_ok(),
            "should build server config: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_missing_key_triggers_regeneration() {
        let dir = tempdir().unwrap();
        let cert_path = dir.path().join("daemon.crt");
        let key_path = dir.path().join("daemon.key");

        // Generate cert so cert_path exists
        let _ = load_or_generate_cert(&cert_path, &key_path).unwrap();
        // Remove the key file — cert_path exists but key_path does not.
        fs::remove_file(&key_path).unwrap();

        // load_or_generate_cert requires both files to exist for the load path;
        // since the pair is incomplete, it falls through to generate a fresh cert.
        let result = load_or_generate_cert(&cert_path, &key_path);
        assert!(result.is_ok(), "should regenerate when key is missing");
    }

    #[test]
    fn test_corrupt_key_file_returns_error() {
        let dir = tempdir().unwrap();
        let cert_path = dir.path().join("daemon.crt");
        let key_path = dir.path().join("daemon.key");

        // Generate a valid cert+key pair first
        let _ = load_or_generate_cert(&cert_path, &key_path).unwrap();

        // Overwrite the key file with garbage — not a valid PEM block
        fs::write(&key_path, b"this is not a PEM key\n").unwrap();

        // Both files exist so the load branch runs, but the key parse fails
        let result = load_or_generate_cert(&cert_path, &key_path);
        assert!(result.is_err(), "corrupt key file should return an error");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no private key"),
            "expected 'no private key' in error, got: {err}"
        );
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! TLS client configuration: rustls with the `ring` provider, certificates checked by the
//! operating system (`rustls-platform-verifier`), ALPN `h2` then `http/1.1`, optionally a
//! client certificate (mTLS, wave 5).
//!
//! Built on the first HTTPS connection, not when the transport is created: on Linux the
//! verifier reads the system CA bundle, which costs start-up time, and a system without CA
//! certificates (e.g. a minimal container) can still send plain `http://` requests.
//!
//! Client certificates: PEM (certificate chain plus a PKCS#8, PKCS#1 or SEC1 key, or an
//! encrypted PKCS#8 key with the passphrase), DER certificates, and PKCS#12 archives
//! (PBES2/AES as well as the legacy PBES1 3DES/RC2 encryption). Problems with the key
//! material are `TransportError::Tls` and never quote it.

use std::sync::{Arc, OnceLock};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls_platform_verifier::Verifier;
use tokio_rustls::TlsConnector;

use crate::transport::{ClientIdentity, TransportError};

pub(super) struct TlsSetup {
    extra_roots: Vec<CertificateDer<'static>>,
    identity: Option<ClientIdentity>,
    connector: OnceLock<Result<TlsConnector, TransportError>>,
}

impl TlsSetup {
    pub fn new(extra_roots: Vec<CertificateDer<'static>>) -> Self {
        Self::with_identity(extra_roots, None)
    }

    /// The same trust, presenting `identity` when the server asks for a certificate.
    pub fn with_identity(
        extra_roots: Vec<CertificateDer<'static>>,
        identity: Option<ClientIdentity>,
    ) -> Self {
        Self {
            extra_roots,
            identity,
            connector: OnceLock::new(),
        }
    }

    pub fn extra_roots(&self) -> &[CertificateDer<'static>] {
        &self.extra_roots
    }

    pub fn connector(&self) -> Result<TlsConnector, TransportError> {
        self.connector
            .get_or_init(|| build(&self.extra_roots, self.identity.as_ref()))
            .clone()
    }
}

fn build(
    extra_roots: &[CertificateDer<'static>],
    identity: Option<&ClientIdentity>,
) -> Result<TlsConnector, TransportError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    #[cfg(not(target_os = "android"))]
    let verifier = if extra_roots.is_empty() {
        Verifier::new(provider.clone())
    } else {
        Verifier::new_with_extra_roots(extra_roots.to_vec(), provider.clone())
    };
    #[cfg(target_os = "android")]
    let verifier = {
        let _ = extra_roots;
        Verifier::new(provider.clone())
    };
    let verifier = verifier.map_err(setup_error)?;
    let builder = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(setup_error)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier));
    let mut config = match identity {
        None => builder.with_no_client_auth(),
        Some(identity) => {
            let (chain, key) = identity_parts(identity)?;
            builder
                .with_client_auth_cert(chain, key)
                .map_err(|e| cert_error(&format!("not usable: {e}")))?
        }
    };
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(TlsConnector::from(Arc::new(config)))
}

fn setup_error(e: rustls::Error) -> TransportError {
    TransportError::Tls(format!("TLS setup: {e}"))
}

fn cert_error(message: &str) -> TransportError {
    TransportError::Tls(format!("client certificate: {message}"))
}

type Identity = (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>);

/// Certificate chain and private key of an identity.
pub(super) fn identity_parts(identity: &ClientIdentity) -> Result<Identity, TransportError> {
    match identity {
        ClientIdentity::Pem {
            certificate,
            private_key,
            passphrase,
        } => {
            let chain = certificates(certificate)?;
            let key = private_key_from_pem(private_key, passphrase.as_deref())?;
            Ok((chain, key))
        }
        ClientIdentity::Pkcs12 {
            archive,
            passphrase,
        } => pkcs12(archive, passphrase),
    }
}

fn is_pem(bytes: &[u8]) -> bool {
    bytes.windows(11).any(|w| w == b"-----BEGIN ")
}

fn certificates(bytes: &[u8]) -> Result<Vec<CertificateDer<'static>>, TransportError> {
    if !is_pem(bytes) {
        // A single DER certificate (`.cer`, `.der`).
        return Ok(vec![CertificateDer::from(bytes.to_vec())]);
    }
    let chain = CertificateDer::pem_slice_iter(bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| cert_error(&format!("the certificate file is not valid PEM ({e})")))?;
    if chain.is_empty() {
        return Err(cert_error(
            "the certificate file has no CERTIFICATE section",
        ));
    }
    Ok(chain)
}

// PEM boundaries of an encrypted PKCS#8 key (RFC 5958), assembled from parts so that secret
// scanners do not take the source for a key.
const ENCRYPTED_BEGIN: &str = concat!("-----BEGIN ENCRYPTED ", "PRIVATE ", "KEY-----");
const ENCRYPTED_END: &str = concat!("-----END ENCRYPTED ", "PRIVATE ", "KEY-----");

fn private_key_from_pem(
    bytes: &[u8],
    passphrase: Option<&str>,
) -> Result<PrivateKeyDer<'static>, TransportError> {
    let text = String::from_utf8_lossy(bytes);
    if text.contains(ENCRYPTED_BEGIN) {
        let Some(passphrase) = passphrase else {
            return Err(cert_error(
                "the private key is encrypted; set the passphrase",
            ));
        };
        return decrypt_pkcs8(&text, passphrase);
    }
    if text.contains("Proc-Type: 4,ENCRYPTED") {
        return Err(cert_error(
            "the private key uses the legacy OpenSSL encryption; convert it to PKCS#8 \
             (`openssl pkcs8 -topk8`) or use PKCS#12",
        ));
    }
    if !is_pem(bytes) {
        return Ok(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            bytes.to_vec(),
        )));
    }
    PrivateKeyDer::from_pem_slice(bytes).map_err(|e| match e {
        rustls::pki_types::pem::Error::NoItemsFound => {
            cert_error("no private key found (expected PRIVATE KEY, RSA or EC PRIVATE KEY)")
        }
        e => cert_error(&format!("the private key is not valid PEM ({e})")),
    })
}

fn decrypt_pkcs8(text: &str, passphrase: &str) -> Result<PrivateKeyDer<'static>, TransportError> {
    use pkcs8::der::Document;

    let start = text.find(ENCRYPTED_BEGIN).unwrap_or(0);
    let end_marker = ENCRYPTED_END;
    let end = text[start..]
        .find(end_marker)
        .map_or(text.len(), |i| start + i + end_marker.len());
    let (_, document) = Document::from_pem(&text[start..end])
        .map_err(|e| cert_error(&format!("the encrypted private key is not valid PEM ({e})")))?;
    let info = pkcs8::EncryptedPrivateKeyInfoRef::try_from(document.as_bytes())
        .map_err(|e| cert_error(&format!("the encrypted private key is invalid ({e})")))?;
    let secret = info.decrypt(passphrase).map_err(|_| {
        cert_error("the private key cannot be decrypted (wrong passphrase or unsupported cipher)")
    })?;
    Ok(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        secret.as_bytes().to_vec(),
    )))
}

fn pkcs12(archive: &[u8], passphrase: &str) -> Result<Identity, TransportError> {
    use p12_keystore::{KeyStore, Pkcs12ImportPolicy};

    let store =
        KeyStore::from_pkcs12(archive, passphrase, Pkcs12ImportPolicy::Strict).map_err(|e| {
            cert_error(&format!(
                "the PKCS#12 file cannot be read (wrong passphrase or damaged file: {e})"
            ))
        })?;
    let Some((_, chain)) = store.private_key_chain() else {
        return Err(cert_error(
            "the PKCS#12 file has no private key with its certificate",
        ));
    };
    let certs = chain
        .certs()
        .iter()
        .map(|c| CertificateDer::from(c.as_der().to_vec()))
        .collect();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(chain.key().as_der().to_vec()));
    Ok((certs, key))
}

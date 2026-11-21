// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! TLS client configuration: rustls with the `ring` provider, certificates checked by the
//! operating system (`rustls-platform-verifier`), ALPN `h2` then `http/1.1`.
//!
//! Built on the first HTTPS connection, not when the transport is created: on Linux the
//! verifier reads the system CA bundle, which costs start-up time, and a system without CA
//! certificates (e.g. a minimal container) can still send plain `http://` requests.

use std::sync::{Arc, OnceLock};

use rustls::pki_types::CertificateDer;
use rustls_platform_verifier::Verifier;
use tokio_rustls::TlsConnector;

use crate::transport::TransportError;

pub(super) struct TlsSetup {
    extra_roots: Vec<CertificateDer<'static>>,
    connector: OnceLock<Result<TlsConnector, TransportError>>,
}

impl TlsSetup {
    pub fn new(extra_roots: Vec<CertificateDer<'static>>) -> Self {
        Self {
            extra_roots,
            connector: OnceLock::new(),
        }
    }

    pub fn connector(&self) -> Result<TlsConnector, TransportError> {
        self.connector
            .get_or_init(|| build(&self.extra_roots))
            .clone()
    }
}

fn build(extra_roots: &[CertificateDer<'static>]) -> Result<TlsConnector, TransportError> {
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
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(setup_error)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(TlsConnector::from(Arc::new(config)))
}

fn setup_error(e: rustls::Error) -> TransportError {
    TransportError::Tls(format!("TLS setup: {e}"))
}

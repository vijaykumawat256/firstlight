//! How server certificates are checked, in one place, for the web client, IMAP and SMTP.
//!
//! On Mac, Windows and iPhone the operating system's own checker decides (it knows the system's trusted authorities
//! and any a company has added). Android's checker can only be reached through Java, so there the app carries the
//! standard Mozilla list of trusted authorities (`webpki-roots`) instead; authorities added on the phone by hand are
//! not trusted.

use crate::{Error, Result};

fn failed<E>(_: E) -> Error {
    Error::Provider("could not set up secure connections".into())
}

/// A TLS client set-up that checks the server's certificate and name.
pub fn client_config() -> Result<rustls::ClientConfig> {
    #[cfg(not(target_os = "android"))]
    {
        use rustls_platform_verifier::BuilderVerifierExt;
        Ok(rustls::ClientConfig::builder()
            .with_platform_verifier()
            .map_err(failed)?
            .with_no_client_auth())
    }
    #[cfg(target_os = "android")]
    {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let _ = failed::<()>;
        Ok(rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_secure_connection_can_be_set_up() {
        assert!(super::client_config().is_ok());
    }
}

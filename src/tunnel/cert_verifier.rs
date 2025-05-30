use std::sync::Arc;

use rats_cert::cert::verify::{
    CertVerifier, ClaimsCheck, CocoVerifyMode, VerifyPolicy, VerifyPolicyOutput,
};
use rustls::Error;
use tokio::sync::Mutex;

use crate::{config::ra::VerifyArgs, tunnel::attestation_result::AttestationResult};

#[derive(Debug)]
pub struct CoCoCommonCertVerifier {
    verify: VerifyArgs,
    attestation_result: Arc<Mutex<Option<AttestationResult>>>,
}

impl CoCoCommonCertVerifier {
    pub fn new(verify: VerifyArgs) -> Self {
        Self {
            verify,
            attestation_result: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn get_attestation_result(&self) -> Option<AttestationResult> {
        (*self.attestation_result.lock().await).clone()
    }

    pub fn verify_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
    ) -> std::result::Result<(), rustls::Error> {
        let attestation_result = self.attestation_result.clone();
        let res = tokio::task::block_in_place(move || {
            CertVerifier::new(VerifyPolicy::Coco {
                verify_mode: CocoVerifyMode::Evidence {
                    as_addr: self.verify.as_addr.to_owned(),
                    as_is_grpc: self.verify.as_is_grpc,
                },
                policy_ids: self.verify.policy_ids.to_owned(),
                trusted_certs_paths: self.verify.trusted_certs_paths.clone(),
                claims_check: ClaimsCheck::Custom(Box::new(move |claims| {
                    *attestation_result.blocking_lock() =
                        Some(AttestationResult::from_claims(claims));
                    // We do not check the claims here, just leave it to be checked by attestation service.
                    VerifyPolicyOutput::Passed
                })),
            })
            .verify_der(end_entity)
        });

        tracing::debug!(result=?res, "rats-rs cert verify finished");

        match res {
            Ok(VerifyPolicyOutput::Passed) => Ok(()),
            Ok(VerifyPolicyOutput::Failed) => Err(Error::General(
                "Verify failed because of claims".to_string(),
            )),
            Err(err) => Err(Error::General(
                format!("Verify failed with err: {:?}", err).to_string(),
            )),
        }
    }
}

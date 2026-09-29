//! Atomic admission of one inbound service's Prepared history and protected input.
//! Uses the same IO history/material SQL and accounting as independent callers.
use super::{Store, boundary, io_evidence, io_intent, sql};
use crate::{
    Error, Result,
    io_evidence::{Kind, Material},
    io_intent::{Phase, Record},
    plugin_package::io::IoCapability,
    service_record::RequestRecord,
};
use rusqlite::TransactionBehavior;
impl Store {
    /// Atomically retain Prepared, follow-up capacity, both material reservations,
    /// and the exact protected RequestRecord. This does not claim dispatch.
    /// Err or unwinding before commit rolls all writes back through Transaction
    /// ownership. CommitUnknown requires a read of original history, never resend.
    pub fn prepare_service_request_local_authorized(
        &mut self,
        prepared: &Record,
        material: &Material,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        let command = prepared.command();
        if prepared.phase() != Phase::Prepared
            || command.capability != IoCapability::HttpPublish
            || command.protocol_sha256 != crate::service_record::schema_digest()
            || material.kind() != Kind::Request
            || material.operation_id() != command.operation_id
            || material.subject() != command.subject
            || material.request_sha256() != command.request_sha256
            || material.payload_sha256() != command.request_sha256
            || material.payload().len() as u64 != command.request_bytes
        {
            return Err(Error::Invalid("service preparation metadata"));
        }
        let request = RequestRecord::decode(material.payload())?;
        prepared.matches_command(&request.command(command.package_sha256)?)?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        let (stored, _) =
            io_intent::append_in_tx(&tx, self.budget, prepared, &mut authorize, false)?;
        io_intent::reserve_followup_in_tx(&tx, self.budget, command, &mut authorize)?;
        io_evidence::reserve_materials_in_tx(&tx, self.budget, command, &mut authorize)?;
        io_evidence::store_material_in_tx(
            &tx,
            &command.subject,
            Kind::Request,
            material,
            &mut authorize,
        )?;
        // No effectful callback may run inside this synchronous authorization.
        // It is checked again after every piece has been admitted, before commit.
        authorize()?;
        boundary("service-request-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("service-request-after-commit");
        Ok(stored)
    }
}

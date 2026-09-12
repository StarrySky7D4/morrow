//! Host-created edit intent over an authentic dependency result, never a guest-selected command.
use crate::dependency::DependencyOutput;
use morrow_core::{
    Error, Result,
    content_change::ContentChange,
    dispatch::{Connection, HostRuntime},
    transaction::Receipt,
};
/// All target metadata comes from the trusted host workflow, separate from plugin output bytes.
pub struct EditTarget<'a> {
    pub operation_id: &'a str,
    pub card_id: &'a str,
    pub expected_revision: u64,
    pub title: &'a str,
    pub preview: &'a str,
    pub accepted_output_type: &'a str,
}
/// Retains the authentic output and its dependency revocation state across preview and commit.
/// The stable operation ID permits explicit receipt lookup/retry; no automatic task rerun occurs.
pub struct EditProposal {
    output: DependencyOutput,
    change: ContentChange,
}
impl EditProposal {
    pub fn new(output: DependencyOutput, target: EditTarget<'_>) -> Result<Self> {
        if output.output_type() != target.accepted_output_type {
            return Err(Error::Invalid("dependency output type"));
        }
        let change = ContentChange {
            operation_id: target.operation_id.into(),
            card_id: target.card_id.into(),
            expected_revision: target.expected_revision,
            title: target.title.into(),
            body: output.bytes().to_vec(),
            preview_text: target.preview.into(),
            attachments: None,
        };
        change.validate()?;
        Ok(Self { output, change })
    }
    pub fn body(&self) -> &[u8] {
        &self.change.body
    }
    pub fn operation_id(&self) -> &str {
        &self.change.operation_id
    }
    pub fn commit(
        &self,
        host: &mut HostRuntime,
        caller: &Connection,
        mut clock: impl FnMut() -> u64,
    ) -> Result<Receipt> {
        self.output
            .validate(host, caller, clock())
            .map_err(|_| Error::Invalid("inactive dependency result"))?;
        host.edit_content_guarded(caller, &self.change, &mut clock, |now| {
            self.output
                .validate_liveness(now)
                .map_err(|_| Error::Invalid("inactive dependency result"))
        })
    }
}

//! Bounded mapping for the private service administration channel only.
use crate::{Result, WorkbenchState, host_capnp as wire, service_control};
use morrow_core::{
    service_authority::proto as authority,
    service_config::{Config, proto as config},
};

pub(crate) fn is_action(action: wire::Action) -> bool {
    matches!(
        action,
        wire::Action::ServiceConfigPage
            | wire::Action::ServiceConfigSave
            | wire::Action::ServiceConfigDisable
            | wire::Action::ServiceAuthorityPage
            | wire::Action::ServiceAuthenticationIssue
            | wire::Action::ServiceAuthorityDisable
            | wire::Action::ServicePublicationSave
            | wire::Action::ServiceTlsInspect
    )
}

fn text(value: capnp::Result<capnp::text::Reader<'_>>, limit: usize) -> Result<String> {
    let value = value?;
    if value.len() > limit {
        return Err("service text limit".into());
    }
    Ok(value.to_str()?.to_owned())
}
fn reference(value: &[u8], optional: bool) -> Result<&[u8]> {
    if (optional && value.is_empty()) || (value.len() == 32 && value.iter().any(|v| *v != 0)) {
        Ok(value)
    } else {
        Err("invalid service reference".into())
    }
}

pub(crate) fn tls_path(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    let value = text(value, 4096)?;
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err("invalid TLS path".into());
    }
    Ok(value)
}
fn principals(
    input: capnp::struct_list::Reader<'_, wire::service_principal::Owned>,
) -> Result<Vec<config::Principal>> {
    if input.len() > 64 {
        return Err("service principal limit".into());
    }
    // Count the entire nested list before allocating any model principals.
    let mut total = 0u32;
    for p in input.iter() {
        total = total
            .checked_add(p.get_scopes()?.len())
            .ok_or("service scope limit")?;
        if total > 128 {
            return Err("service scope limit".into());
        }
    }
    input
        .iter()
        .map(|p| {
            let scopes = p.get_scopes()?;
            Ok(config::Principal {
                id: text(p.get_id(), 128)?,
                authentication_reference: reference(p.get_authentication_reference()?, false)?
                    .to_vec(),
                content_scopes: scopes
                    .iter()
                    .map(|s| {
                        Ok(config::ContentScope {
                            kind: i32::from(s.get_kind()),
                            card_id: text(s.get_card_id(), 256)?,
                            attachment_id: text(s.get_attachment_id(), 256)?,
                        })
                    })
                    .collect::<Result<_>>()?,
            })
        })
        .collect()
}
fn config_update(
    value: wire::service_config_update::Reader<'_>,
) -> Result<service_control::ServiceConfigUpdate> {
    Ok(service_control::ServiceConfigUpdate {
        id: text(value.get_id(), 256)?,
        expected_revision: value.get_expected_revision(),
        registry_revision: value.get_registry_revision(),
        package_id: text(value.get_package_id(), 256)?,
        package_digest: reference(value.get_package_digest()?, false)?.try_into()?,
        service: text(value.get_service(), 256)?,
        handler: text(value.get_handler(), 256)?,
        retention_ms: value.get_retention_ms(),
        principals: principals(value.get_principals()?)?,
    })
}
fn publication_update(
    value: wire::service_publication_update::Reader<'_>,
) -> Result<service_control::PublicationUpdate> {
    let p = value.get_policy()?;
    Ok(service_control::PublicationUpdate {
        reference: reference(value.get_reference()?, false)?.try_into()?,
        expected_revision: value.get_expected_revision(),
        config_revision: value.get_config_revision(),
        config_digest: reference(p.get_config_digest()?, false)?.try_into()?,
        config_id: text(p.get_config_id(), 256)?,
        registry_revision: value.get_registry_revision(),
        package_id: text(value.get_package_id(), 256)?,
        lifetime_days: value.get_lifetime_days(),
        listen_address: text(p.get_listen_address(), 64)?,
        tls_required: p.get_tls_required(),
        method: text(p.get_method(), 32)?,
        path: text(p.get_path(), 8192)?,
        query_path: text(p.get_query_path(), 8192)?,
    })
}
fn write_principals(
    values: &[config::Principal],
    mut out: capnp::struct_list::Builder<'_, wire::service_principal::Owned>,
) -> Result<()> {
    if values.len() > 64 || values.iter().map(|v| v.content_scopes.len()).sum::<usize>() > 128 {
        return Err("service principal or scope limit".into());
    }
    for (i, value) in values.iter().enumerate() {
        let mut p = out.reborrow().get(i as u32);
        p.set_id(&value.id);
        p.set_authentication_reference(&value.authentication_reference);
        let mut scopes = p.init_scopes(value.content_scopes.len().try_into()?);
        for (i, value) in value.content_scopes.iter().enumerate() {
            let mut s = scopes.reborrow().get(i as u32);
            s.set_kind(value.kind.try_into()?);
            s.set_card_id(&value.card_id);
            s.set_attachment_id(&value.attachment_id);
        }
    }
    Ok(())
}
fn write_config(config: &Config, mut out: wire::service_config_info::Builder<'_>) -> Result<()> {
    let value = config.value();
    if value.principals.len() > 64 || value.approval_references.len() > 64 {
        return Err("service configuration limit".into());
    }
    out.set_id(&value.id);
    out.set_revision(value.revision);
    out.set_namespace(&value.namespace);
    out.set_retention_ms(value.retention_ms);
    out.set_service(&value.service);
    out.set_handler(&value.handler);
    out.set_package_digest(&value.package_sha256);
    out.set_disabled(value.disabled);
    out.set_digest(&config.digest());
    write_principals(
        &value.principals,
        out.reborrow()
            .init_principals(value.principals.len().try_into()?),
    )?;
    let mut refs = out.init_approval_references(value.approval_references.len().try_into()?);
    for (i, reference) in value.approval_references.iter().enumerate() {
        refs.set(i as u32, reference);
    }
    Ok(())
}
fn write_publication(
    value: &authority::Publication,
    mut out: wire::service_publication::Builder<'_>,
) {
    out.set_config_id(&value.config_id);
    out.set_config_digest(&value.config_sha256);
    out.set_listen_address(&value.listen_address);
    out.set_tls_required(value.tls_required);
    out.set_method(&value.method);
    out.set_path(&value.path);
    out.set_query_path(&value.query_path);
}
fn write_authority(
    value: &service_control::AuthorityInfo,
    mut out: wire::service_authority_info::Builder<'_>,
) {
    out.set_reference(&value.reference);
    out.set_revision(value.revision);
    out.set_created_ms(value.created_ms);
    out.set_expires_ms(value.expires_ms);
    out.set_disabled(value.disabled);
    match &value.kind {
        service_control::AuthorityKind::Authentication { principal_id } => {
            out.set_kind(1);
            out.set_principal_id(principal_id);
        }
        service_control::AuthorityKind::Publication(value) => {
            out.set_kind(2);
            write_publication(value, out.init_publication());
        }
    }
}

pub(crate) fn handle(
    host: &mut WorkbenchState,
    request: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    match request.get_action()? {
        wire::Action::ServiceTlsInspect => {
            let selected = request.get_service_tls()?;
            let certificate = tls_path(selected.get_certificate_path())?;
            let private_key = tls_path(selected.get_private_key_path())?;
            let checked = crate::service_tls::TlsSelection::inspect(
                std::path::Path::new(&certificate),
                std::path::Path::new(&private_key),
            )?;
            let mut reply = out.init_service_tls();
            reply.set_certificate_path(&certificate);
            reply.set_private_key_path(&private_key);
            reply.set_certificate_sha256(&checked.certificate_sha256());
        }
        wire::Action::ServiceConfigPage => {
            let page = host.service_config_page(
                &text(request.get_cursor(), 256)?,
                reference(request.get_service_snapshot()?, true)?,
            )?;
            if page.configs.len() > 1 {
                return Err("service config page limit".into());
            }
            out.set_service_snapshot(&page.snapshot);
            out.set_cursor(page.next.as_deref().unwrap_or(""));
            let mut items = out.init_service_configs(page.configs.len().try_into()?);
            for (i, value) in page.configs.iter().enumerate() {
                write_config(value, items.reborrow().get(i as u32))?;
            }
        }
        wire::Action::ServiceConfigSave => {
            let value = host.save_service_config(config_update(request.get_service_config()?)?)?;
            write_config(&value, out.init_service_configs(1).get(0))?;
        }
        wire::Action::ServiceConfigDisable => {
            let value =
                host.disable_service_config(&text(request.get_id(), 256)?, request.get_revision())?;
            write_config(&value, out.init_service_configs(1).get(0))?;
        }
        wire::Action::ServiceAuthorityPage => {
            let page = host.service_authority_page(
                reference(request.get_service_cursor()?, true)?,
                reference(request.get_service_snapshot()?, true)?,
            )?;
            if page.entries.len() > 2 {
                return Err("service authority page limit".into());
            }
            out.set_service_snapshot(&page.snapshot);
            out.set_service_cursor(page.next.as_ref().map_or(&[], |v| v.as_slice()));
            let mut items = out.init_service_authorities(page.entries.len().try_into()?);
            for (i, value) in page.entries.iter().enumerate() {
                write_authority(value, items.reborrow().get(i as u32));
            }
        }
        wire::Action::ServiceAuthenticationIssue => {
            let issued = host.issue_service_authentication(
                reference(request.get_service_reference()?, true)?,
                request.get_revision(),
                &text(request.get_principal_id(), 128)?,
                request.get_service_days(),
            )?;
            // Perform every fallible step before copying the one-time token.
            if issued.token.len() != 64 || !issued.token.bytes().all(|v| v.is_ascii_hexdigit()) {
                return Err("invalid issued service token".into());
            }
            write_authority(
                &issued.info,
                out.reborrow().init_service_authorities(1).get(0),
            );
            out.set_issued_token(issued.token.as_bytes());
        }
        wire::Action::ServiceAuthorityDisable => {
            let value = host.disable_service_authority(
                reference(request.get_service_reference()?, false)?,
                request.get_revision(),
            )?;
            write_authority(&value, out.init_service_authorities(1).get(0));
        }
        wire::Action::ServicePublicationSave => {
            let value = host.save_service_publication(publication_update(
                request.get_service_publication()?,
            )?)?;
            write_authority(&value, out.init_service_authorities(1).get(0));
        }
        _ => return Err("not a service administration action".into()),
    }
    Ok(())
}

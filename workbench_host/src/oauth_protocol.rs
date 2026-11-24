use super::{text, wire};
use crate::{
    Result, Workbench,
    oauth_accounts::accounts::{State, Status},
    oauth_control::{Phase, Selection},
};
fn id(bytes: &[u8]) -> Result<[u8; 32]> {
    let value: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "invalid account control identity")?;
    if value == [0; 32] {
        return Err("empty account control identity".into());
    }
    Ok(value)
}
fn optional(bytes: &[u8]) -> Result<Option<[u8; 32]>> {
    if bytes.is_empty() {
        Ok(None)
    } else {
        Ok(Some(id(bytes)?))
    }
}
fn account(status: Status, mut out: wire::o_auth_account::Builder<'_>) {
    out.set_reference(&status.reference);
    out.set_revision(status.revision);
    out.set_state(match status.state {
        State::Ready => 1,
        State::Refreshing => 2,
        State::SignInRequired => 3,
        State::SignedOut => 4,
    });
    out.set_access_expires_ms(status.access_expires_ms.unwrap_or_default());
}
pub(super) fn handle(
    host: &mut Workbench,
    r: wire::request::Reader<'_>,
    out: wire::response::Builder<'_>,
) -> Result<()> {
    if !r.has_oauth_control() {
        return Err("missing account control".into());
    }
    let p = r.get_oauth_control()?;
    let sel = Selection {
        package: text(r.get_id())?,
        digest: id(r.get_sha256()?)?,
        revision: r.get_revision(),
    };
    let mut out = out.init_oauth_view();
    use wire::OAuthAction as A;
    let action = p.get_action()?;
    match action {
        A::Services => {
            let services = host.oauth_services(&sel)?;
            let mut list = out.init_services(services.len().try_into()?);
            for (i, s) in services.into_iter().enumerate() {
                let mut row = list.reborrow().get(i as u32);
                row.set_key(&s.key);
                row.set_name(s.name.as_str());
                row.set_issuer(s.issuer.as_str());
                row.set_authorize(s.authorize.as_str());
                row.set_token_endpoint(s.token.as_str());
                row.set_resource(s.resource.as_str());
                row.set_client_id(s.client_id.as_str());
                row.set_scopes(s.scopes.as_str());
            }
        }
        A::Accounts => {
            let (rows, snapshot, next) = host.oauth_account_page(
                &sel,
                id(p.get_service()?)?,
                optional(p.get_cursor()?)?,
                optional(p.get_snapshot()?)?,
            )?;
            out.set_snapshot(&snapshot);
            if let Some(next) = next {
                out.set_next(&next);
            }
            let mut list = out.init_accounts(rows.len().try_into()?);
            for (i, row) in rows.into_iter().enumerate() {
                account(row, list.reborrow().get(i as u32));
            }
        }
        A::Logout => {
            let service = id(p.get_service()?)?;
            let reference = id(p.get_account()?)?;
            let revision = p.get_expected_revision();
            if let Some(key) = optional(p.get_key()?)? {
                return write_work(
                    host.oauth_withdraw(&sel, service, reference, revision, key)?,
                    out,
                );
            }
            account(
                host.oauth_sign_out(&sel, service, reference, revision)?,
                out.init_accounts(1).get(0),
            );
        }
        A::Login | A::Refresh | A::Poll | A::Cancel => {
            let service = id(p.get_service()?)?;
            let key = id(p.get_key()?)?;
            let view = match action {
                A::Login => host.oauth_start(&sel, service, None, key, p.get_days())?,
                A::Refresh => {
                    host.oauth_start(&sel, service, Some(id(p.get_account()?)?), key, 0)?
                }
                _ => host.oauth_poll(&sel, service, key, action == A::Cancel)?,
            };
            return write_work(view, out);
        }
    }
    Ok(())
}
fn write_work(
    view: crate::oauth_control::Work,
    mut out: wire::o_auth_view::Builder<'_>,
) -> Result<()> {
    out.set_key(&view.key);
    if let Some(account) = view.account {
        out.set_account(&account);
    }
    out.set_phase(match view.phase {
        Phase::Waiting => 1,
        Phase::Done => 2,
        Phase::Cancelled => 3,
        Phase::SignInRequired => 4,
        Phase::Unconfirmed => 5,
    });
    if let Some(url) = view.browser_url {
        out.set_browser_url(url.as_str());
    }
    if let Some(status) = view.status {
        account(status, out.init_accounts(1).get(0));
    }
    Ok(())
}

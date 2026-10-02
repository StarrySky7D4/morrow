//! Secret Service stores the protected secret; files contain only unpredictable,
//! domain-bound item references. No plaintext seed or key-encryption key is saved
//! in a file. Only an already running same-UID Unix session service is permitted.
use super::{KeyError, Result};
use std::{
    collections::HashMap,
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::net::UnixStream,
    },
    path::{Path, PathBuf},
    time::Duration,
};
use zbus::{
    blocking::{Connection, Proxy},
    proxy::{CacheProperties, MethodFlags},
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};
use zeroize::{Zeroize, Zeroizing};

#[cfg(all(feature = "linux-test-keyring", not(debug_assertions)))]
compile_error!(
    "linux-test-keyring is a debug-only synthetic fixture and must never be built in release"
);
const NAME: &str = "org.freedesktop.secrets";
const SERVICE: &str = "org.freedesktop.Secret.Service";
const COLLECTION: &str = "org.freedesktop.Secret.Collection";
const ITEM: &str = "org.freedesktop.Secret.Item";
const ROOT: &str = "/org/freedesktop/secrets";
const MAX_SECRET: usize = 256 * 1024;
const REFERENCE_MAGIC: &[u8; 8] = b"MORROWS1";
const AUDIT: &str = "morrow-audit-key-v1";
const HTTP: &str = "morrow-http-credential-v1";
const TLS: &str = "morrow-tls-identity-v1";

pub(super) fn random(bytes: &mut [u8]) -> Result<()> {
    getrandom::fill(bytes).map_err(|_| KeyError::Random)
}
fn protection<T>(r: zbus::Result<T>) -> Result<T> {
    r.map_err(|_| KeyError::Protection)
}
macro_rules! call {
    ($proxy:expr, $method:expr, $body:expr, $out:ty) => {
        protection($proxy.call_with_flags::<_, _, $out>(
            $method,
            MethodFlags::NoAutoStart.into(),
            $body,
        ))?
        .ok_or(KeyError::Protection)
    };
}
/// Reject alternatives, TCP, abstract names, duplicate parameters, encoded path
/// separators and autolaunch before attempting any connection/authentication.
fn unix_address(address: &str) -> Result<PathBuf> {
    if address.len() > 4096
        || address.contains(';')
        || address.contains('%')
        || address.contains('\0')
    {
        return Err(KeyError::Protection);
    }
    let address = address.strip_prefix("unix:").ok_or(KeyError::Protection)?;
    let mut path = None;
    let mut guid = false;
    for component in address.split(',') {
        if let Some(p) = component.strip_prefix("path=") {
            if path.is_some()
                || !Path::new(p).is_absolute()
                || Path::new(p).components().any(|v| {
                    matches!(
                        v,
                        std::path::Component::ParentDir | std::path::Component::CurDir
                    )
                })
                || p.len() > 107
                || p.contains('\n')
                || p.contains('\r')
            {
                return Err(KeyError::Protection);
            }
            path = Some(PathBuf::from(p));
        } else if let Some(value) = component.strip_prefix("guid=") {
            if guid || value.len() != 32 || !value.bytes().all(|v| v.is_ascii_hexdigit()) {
                return Err(KeyError::Protection);
            }
            guid = true;
        } else {
            return Err(KeyError::Protection);
        }
    }
    path.ok_or(KeyError::Protection)
}
fn connect_unix(path: &Path) -> Result<UnixStream> {
    // A full listener backlog must fail closed immediately instead of blocking
    // connect forever. No retry, alternate address, autolaunch, or fallback.
    let fd = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            0,
        )
    };
    if fd < 0 {
        return Err(KeyError::Protection);
    }
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    use std::os::unix::ffi::OsStrExt;
    let bytes = path.as_os_str().as_bytes();
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    if bytes.is_empty() || bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err(KeyError::Protection);
    }
    for (out, byte) in address.sun_path.iter_mut().zip(bytes) {
        *out = *byte as libc::c_char;
    }
    let status = unsafe {
        libc::connect(
            fd,
            (&address as *const libc::sockaddr_un).cast(),
            std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
        )
    };
    if status != 0 {
        return Err(KeyError::Protection);
    }
    Ok(stream)
}
fn bounded<T>(
    future: impl std::future::Future<Output = zbus::Result<T>>,
    duration: Duration,
) -> zbus::Result<T> {
    futures_lite::future::block_on(futures_lite::future::race(future, async move {
        async_io::Timer::after(duration).await;
        Err(zbus::Error::Failure(
            "bounded Secret Service connection timeout".into(),
        ))
    }))
}
fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    let mut peer = std::mem::MaybeUninit::<libc::ucred>::uninit();
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            peer.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    if len as usize != std::mem::size_of::<libc::ucred>() {
        return Err(io::Error::other("invalid peer credentials"));
    }
    let peer = unsafe { peer.assume_init() };
    if peer.pid <= 0 {
        return Err(io::Error::other("invalid peer credentials"));
    }
    Ok(peer.uid)
}
fn same_uid(actual: u32, expected: u32) -> Result<()> {
    if actual != expected {
        Err(KeyError::Protection)
    } else {
        Ok(())
    }
}
fn proxy<'a>(
    connection: &Connection,
    owner: &'a str,
    path: &'a str,
    interface: &'a str,
) -> Result<Proxy<'a>> {
    let builder = zbus::blocking::proxy::Builder::<Proxy<'a>>::new(connection)
        .cache_properties(CacheProperties::No);
    protection(
        protection(protection(builder.destination(owner))?.path(path))?.interface(interface),
    )?
    .build()
    .map_err(|_| KeyError::Protection)
}
fn owner_policy(pinned: &str, current: &str, actual_uid: u32, expected_uid: u32) -> Result<()> {
    if pinned != current {
        return Err(KeyError::Protection);
    }
    same_uid(actual_uid, expected_uid)
}
fn guarded<T>(
    verify: impl FnOnce() -> Result<()>,
    material: impl FnOnce() -> Result<T>,
) -> Result<T> {
    verify()?;
    material()
}
fn verify_current_owner(connection: &Connection, pinned: &str, uid: u32) -> Result<()> {
    same_uid(morrow_core::linux_storage::current_uid(), uid)?;
    let bus = proxy(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    let current: String = call!(bus, "GetNameOwner", &(NAME,), String)?;
    if current != pinned {
        return Err(KeyError::Protection);
    }
    let actual_uid: u32 = call!(bus, "GetConnectionUnixUser", &(pinned,), u32)?;
    owner_policy(pinned, &current, actual_uid, uid)
}
macro_rules! material {
    ($self:expr, $proxy:expr, $method:expr, $body:expr, $out:ty) => {
        $self.guarded(|| call!($proxy, $method, $body, $out))
    };
}
struct Service {
    connection: Connection,
    owner: String,
    expected_uid: u32,
    session: OwnedObjectPath,
}
impl Service {
    fn connect() -> Result<Self> {
        let uid = morrow_core::linux_storage::current_uid();
        if unsafe { libc::getuid() } != uid {
            return Err(KeyError::Protection);
        }
        let address =
            std::env::var("DBUS_SESSION_BUS_ADDRESS").map_err(|_| KeyError::Protection)?;
        let path = unix_address(&address)?;
        let stream = connect_unix(&path)?;
        // Critical ordering: SO_PEERCRED on the actual connected stream before
        // zbus sees it, before AUTH, Hello, owner queries, or any secret message.
        same_uid(peer_uid(&stream).map_err(|_| KeyError::Protection)?, uid)?;
        // Bound the complete AUTH/Hello builder future. method_timeout alone
        // applies only after the connection has already been established.
        let builder = zbus::connection::Builder::async_io_unix_stream(stream)
            .auth_mechanism(zbus::connection::AuthMechanism::External)
            .method_timeout(Duration::from_secs(5))
            .max_queued(16);
        let connection: Connection =
            protection(bounded(builder.build(), Duration::from_secs(5)))?.into();
        let bus = proxy(
            &connection,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )?;
        let owner: String = call!(bus, "GetNameOwner", &(NAME,), String)?;
        if !owner.starts_with(':') {
            return Err(KeyError::Protection);
        }
        let owner_uid: u32 = call!(bus, "GetConnectionUnixUser", &(owner.as_str(),), u32)?;
        same_uid(owner_uid, uid)?;
        // Pin unique owner on every call. Never ask D-Bus to autostart a service.
        let service = proxy(&connection, &owner, ROOT, SERVICE)?;
        let (output, session): (OwnedValue, OwnedObjectPath) = guarded(
            || verify_current_owner(&connection, &owner, uid),
            || {
                call!(
                    service,
                    "OpenSession",
                    &("plain", Value::from("")),
                    (OwnedValue, OwnedObjectPath)
                )
            },
        )?;
        if session.as_str() == "/"
            || <String>::try_from(output).map_err(|_| KeyError::Protection)? != ""
        {
            return Err(KeyError::Protection);
        }
        drop(service);
        drop(bus);
        Ok(Self {
            connection,
            owner,
            expected_uid: uid,
            session,
        })
    }
    fn guarded<T>(&self, material: impl FnOnce() -> Result<T>) -> Result<T> {
        guarded(
            || verify_current_owner(&self.connection, &self.owner, self.expected_uid),
            material,
        )
    }
    fn attrs(domain: &str, id: &[u8; 32]) -> HashMap<String, String> {
        HashMap::from([
            ("application".into(), "morrow".into()),
            ("schema".into(), "1".into()),
            ("domain".into(), domain.into()),
            (
                "uid".into(),
                morrow_core::linux_storage::current_uid().to_string(),
            ),
            (
                "reference".into(),
                id.iter().map(|b| format!("{b:02x}")).collect(),
            ),
        ])
    }
    fn property(&self, path: &str, interface: &str, property: &str) -> Result<OwnedValue> {
        let p = proxy(
            &self.connection,
            &self.owner,
            path,
            "org.freedesktop.DBus.Properties",
        )?;
        material!(self, p, "Get", &(interface, property), OwnedValue)
    }
    fn search(&self, attrs: &HashMap<String, String>) -> Result<Vec<OwnedObjectPath>> {
        let service = proxy(&self.connection, &self.owner, ROOT, SERVICE)?;
        let (unlocked, locked): (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) = material!(
            self,
            service,
            "SearchItems",
            &(attrs,),
            (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>)
        )?;
        if !locked.is_empty() || unlocked.len() > 1 {
            return Err(KeyError::Protection);
        }
        Ok(unlocked)
    }
    fn store(&self, domain: &str, id: &[u8; 32], input: &[u8]) -> Result<()> {
        let attrs = Self::attrs(domain, id);
        if !self.search(&attrs)?.is_empty() {
            return Err(KeyError::Protection);
        }
        let service = proxy(&self.connection, &self.owner, ROOT, SERVICE)?;
        let collection: OwnedObjectPath =
            material!(self, service, "ReadAlias", &("default",), OwnedObjectPath)?;
        if collection.as_str() == "/"
            || <bool>::try_from(self.property(collection.as_str(), COLLECTION, "Locked")?)
                .map_err(|_| KeyError::Protection)?
        {
            return Err(KeyError::Protection);
        }
        let properties: HashMap<&str, Value<'_>> = HashMap::from([
            (
                "org.freedesktop.Secret.Item.Label",
                Value::from("Morrow protected material"),
            ),
            ("org.freedesktop.Secret.Item.Attributes", Value::from(attrs)),
        ]);
        let p = proxy(
            &self.connection,
            &self.owner,
            collection.as_str(),
            COLLECTION,
        )?;
        let secret = (
            &self.session,
            Vec::<u8>::new(),
            input,
            "application/octet-stream",
        );
        let (item, prompt): (OwnedObjectPath, OwnedObjectPath) = material!(
            self,
            p,
            "CreateItem",
            &(properties, secret, false),
            (OwnedObjectPath, OwnedObjectPath)
        )?;
        if item.as_str() == "/" || prompt.as_str() != "/" {
            return Err(KeyError::Protection);
        }
        // Read back the exact published item before returning a reference.
        let verified = self.load(domain, id)?;
        if verified.as_slice() != input {
            return Err(KeyError::Protection);
        }
        Ok(())
    }
    fn load(&self, domain: &str, id: &[u8; 32]) -> Result<Zeroizing<Vec<u8>>> {
        let attrs = Self::attrs(domain, id);
        let items = self.search(&attrs)?;
        let [item] = items.as_slice() else {
            return Err(KeyError::Protection);
        };
        if <bool>::try_from(self.property(item.as_str(), ITEM, "Locked")?)
            .map_err(|_| KeyError::Protection)?
        {
            return Err(KeyError::Protection);
        }
        let actual: HashMap<String, String> = self
            .property(item.as_str(), ITEM, "Attributes")?
            .try_into()
            .map_err(|_| KeyError::Protection)?;
        if actual != attrs {
            return Err(KeyError::Protection);
        }
        let p = proxy(&self.connection, &self.owner, item.as_str(), ITEM)?;
        let (session, parameters, mut value, content_type): (
            OwnedObjectPath,
            Vec<u8>,
            Vec<u8>,
            String,
        ) = material!(
            self,
            p,
            "GetSecret",
            &(&self.session,),
            (OwnedObjectPath, Vec<u8>, Vec<u8>, String)
        )?;
        if session != self.session
            || !parameters.is_empty()
            || value.is_empty()
            || value.len() > MAX_SECRET
            || content_type != "application/octet-stream"
        {
            value.zeroize();
            return Err(KeyError::Protection);
        }
        Ok(Zeroizing::new(value))
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        if let Ok(p) = proxy(
            &self.connection,
            &self.owner,
            self.session.as_str(),
            "org.freedesktop.Secret.Session",
        ) {
            let _ = self.guarded(|| {
                protection(p.call_with_flags::<_, _, ()>(
                    "Close",
                    MethodFlags::NoAutoStart.into(),
                    &(),
                ))
                .map(|_| ())
            });
        }
    }
}
fn protect_domain(input: &[u8], domain: &str) -> Result<Vec<u8>> {
    if input.is_empty() || input.len() > MAX_SECRET {
        return Err(KeyError::Format);
    }
    let mut id = [0u8; 32];
    random(&mut id)?;
    #[cfg(feature = "linux-test-keyring")]
    if test_keyring::store(domain, id, input)? {
        return Ok(reference(id));
    }
    // A failed key-file publication can leave this newly provisioned item
    // orphaned. Do not delete by a broad label/search or rotate another item:
    // recovery/cleanup needs a separately authorized provider operation.
    Service::connect()?.store(domain, &id, input)?;
    Ok(reference(id))
}
fn reference(id: [u8; 32]) -> Vec<u8> {
    [REFERENCE_MAGIC.as_slice(), &id].concat()
}
fn unprotect_domain(input: &[u8], domain: &str) -> Result<Zeroizing<Vec<u8>>> {
    if input.len() != 40 || &input[..8] != REFERENCE_MAGIC {
        return Err(KeyError::Format);
    }
    let id: [u8; 32] = input[8..].try_into().map_err(|_| KeyError::Format)?;
    #[cfg(feature = "linux-test-keyring")]
    if let Some(value) = test_keyring::load(domain, &id)? {
        return Ok(value);
    }
    Service::connect()?.load(domain, &id)
}
pub(super) fn protect(input: &[u8]) -> Result<Vec<u8>> {
    protect_domain(input, AUDIT)
}
pub(super) fn unprotect(input: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    unprotect_domain(input, AUDIT)
}
pub(super) fn protect_http(input: &[u8]) -> Result<Vec<u8>> {
    protect_domain(input, HTTP)
}
pub(super) fn unprotect_http(input: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    unprotect_domain(input, HTTP)
}
pub(super) fn protect_tls(input: &[u8]) -> Result<Vec<u8>> {
    protect_domain(input, TLS)
}
pub(super) fn unprotect_tls(input: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    unprotect_domain(input, TLS)
}

#[cfg(feature = "linux-test-keyring")]
pub mod test_keyring {
    //! Explicit synthetic fixture. Never enabled by environment or production fallback.
    use super::*;
    use std::{cell::RefCell, marker::PhantomData, rc::Rc};
    type Secrets = HashMap<(String, [u8; 32]), Zeroizing<Vec<u8>>>;
    thread_local! { static SECRETS: RefCell<Option<Secrets>> = const { RefCell::new(None) }; }
    pub struct Fixture {
        _thread: PhantomData<Rc<()>>,
    }
    pub fn activate() -> Result<Fixture> {
        SECRETS.with(|secrets| {
            let mut secrets = secrets.borrow_mut();
            if secrets.is_some() {
                return Err(KeyError::Protection);
            }
            *secrets = Some(HashMap::new());
            Ok(Fixture {
                _thread: PhantomData,
            })
        })
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            SECRETS.with(|s| {
                s.borrow_mut().take();
            });
        }
    }
    pub(super) fn store(domain: &str, id: [u8; 32], value: &[u8]) -> Result<bool> {
        SECRETS.with(|s| {
            let mut s = s.borrow_mut();
            let Some(s) = s.as_mut() else {
                return Ok(false);
            };
            s.insert((domain.into(), id), Zeroizing::new(value.to_vec()));
            Ok(true)
        })
    }
    pub(super) fn load(domain: &str, id: &[u8; 32]) -> Result<Option<Zeroizing<Vec<u8>>>> {
        SECRETS.with(|s| {
            let s = s.borrow();
            let Some(s) = s.as_ref() else {
                return Ok(None);
            };
            s.get(&(domain.into(), *id))
                .map(|v| Some(Zeroizing::new(v.to_vec())))
                .ok_or(KeyError::Protection)
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nonlocal_ambiguous_or_autolaunch_bus_addresses_fail_closed() {
        for address in [
            "",
            "autolaunch:",
            "tcp:host=localhost,port=1",
            "unix:abstract=abc",
            "unix:path=relative",
            "unix:path=/tmp/a;unix:path=/tmp/b",
            "unix:path=/tmp/a,path=/tmp/b",
            "unix:path=/tmp/%2fa",
            "unix:path=/tmp/a,extra=value",
            "unix:path=",
            "unix:path=/tmp/a,guid=bad",
        ] {
            assert!(unix_address(address).is_err(), "{address}");
        }
        assert_eq!(
            unix_address("unix:path=/run/user/1000/bus").unwrap(),
            PathBuf::from("/run/user/1000/bus")
        );
        assert!(same_uid(0, 1000).is_err());
        assert!(same_uid(1000, 1000).is_ok());
    }
    #[test]
    fn synthetic_never_ready_handshake_future_times_out() {
        let start = std::time::Instant::now();
        let result: zbus::Result<()> = bounded(std::future::pending(), Duration::from_millis(20));
        assert!(result.is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        // No real socket/Secret Service is exercised by this future-only test.
    }
    #[test]
    fn owner_churn_and_uid_change_reject_before_material_call() {
        let calls = std::cell::Cell::new(0);
        for (owner, uid) in [(":1.11", 1000), (":1.10", 0)] {
            let result = guarded(
                || owner_policy(":1.10", owner, uid, 1000),
                || {
                    calls.set(calls.get() + 1);
                    Ok(())
                },
            );
            assert!(result.is_err());
            assert_eq!(calls.get(), 0);
        }
        let missing = guarded(
            || Err(KeyError::Protection),
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
        );
        assert!(missing.is_err());
        assert_eq!(calls.get(), 0);
        guarded(
            || owner_policy(":1.10", ":1.10", 1000, 1000),
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls.get(), 1);
    }
    #[test]
    fn invalid_reference_and_bounds_fail_before_any_dbus_call() {
        for bytes in [vec![], vec![0; 40], vec![0; 41]] {
            assert!(unprotect(&bytes).is_err());
        }
        assert!(protect(&[]).is_err());
        assert!(protect(&vec![1; MAX_SECRET + 1]).is_err());
    }
    #[cfg(feature = "linux-test-keyring")]
    #[test]
    fn fixture_is_explicit_domain_bound_and_references_contain_no_secret() {
        let _fixture = test_keyring::activate().unwrap();
        let secret = Zeroizing::new(b"synthetic-secret-material-must-not-persist".to_vec());
        let reference = protect(&secret).unwrap();
        assert_eq!(reference.len(), 40);
        assert!(
            !reference
                .windows(secret.len())
                .any(|b| b == secret.as_slice())
        );
        assert_eq!(unprotect(&reference).unwrap().as_slice(), secret.as_slice());
        assert!(unprotect_http(&reference).is_err());
        assert!(unprotect_tls(&reference).is_err());
        let http = protect_http(&secret).unwrap();
        assert!(unprotect(&http).is_err());
        assert_eq!(unprotect_http(&http).unwrap().as_slice(), secret.as_slice());
    }
}

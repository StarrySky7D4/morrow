use codex_exec_server::{
    ExecServerError, HttpClient, HttpRequestParams, HttpRequestResponse, HttpResponseBodyStream,
};
use codex_file_system::*;
use codex_utils_path_uri::PathUri;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

#[derive(Default)]
pub struct DenyCapabilities(pub Mutex<Vec<&'static str>>);
impl DenyCapabilities {
    fn deny<T>(&self, operation: &'static str) -> ExecutorFileSystemFuture<'_, T> {
        self.0.lock().unwrap().push(operation);
        Box::pin(async {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "qualification filesystem unavailable",
            ))
        })
    }
}
macro_rules! fs_deny {
    ($name:ident, $out:ty $(, $arg:ident : $ty:ty)*) => {
        fn $name<'a>(&'a self, _path: &'a PathUri, $($arg: $ty,)* _sandbox: Option<&'a FileSystemSandboxContext>) -> ExecutorFileSystemFuture<'a, $out> {
            self.deny(stringify!($name))
        }
    };
}
impl ExecutorFileSystem for DenyCapabilities {
    fs_deny!(canonicalize, PathUri);
    fs_deny!(read_file, Vec<u8>, _options: ReadFileOptions);
    fs_deny!(read_file_stream, FileSystemReadStream);
    fs_deny!(write_file, (), _contents: Vec<u8>, _options: WriteFileOptions);
    fs_deny!(create_directory, (), _options: CreateDirectoryOptions);
    fs_deny!(get_metadata, FileMetadata, _options: GetMetadataOptions);
    fs_deny!(read_directory, Vec<ReadDirectoryEntry>);
    fs_deny!(walk, WalkOutcome, _options: WalkOptions);
    fs_deny!(remove, (), _options: RemoveOptions);
    fs_deny!(copy, (), _destination: &'a PathUri, _options: CopyOptions);
}
type HttpFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ExecServerError>> + Send + 'a>>;
impl HttpClient for DenyCapabilities {
    fn http_request(&self, _params: HttpRequestParams) -> HttpFuture<'_, HttpRequestResponse> {
        self.0.lock().unwrap().push("http_request");
        Box::pin(async {
            Err(ExecServerError::Protocol(
                "qualification HTTP unavailable".into(),
            ))
        })
    }
    fn http_request_stream(
        &self,
        _params: HttpRequestParams,
    ) -> HttpFuture<'_, (HttpRequestResponse, HttpResponseBodyStream)> {
        self.0.lock().unwrap().push("http_request_stream");
        Box::pin(async {
            Err(ExecServerError::Protocol(
                "qualification HTTP unavailable".into(),
            ))
        })
    }
}

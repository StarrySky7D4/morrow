#include <napi/native_api.h>
#include <arkui/native_interface.h>
#include <arkui/native_node.h>
#include <arkui/native_node_napi.h>
#include <map>
#include <memory>
#include <atomic>
#include <cerrno>
#include <cmath>
#include <cstdint>
#include <fcntl.h>
#include <limits>
#include <string>
#include <unistd.h>
#include <utility>
#include <vector>

extern "C" char *morrow_hmos_request(const char *);
extern "C" char *morrow_hmos_editor_field(const char *);
// Each Rust entry point consumes every owned FD even when validation or I/O
// fails. Caller FDs are synchronously duplicated before NAPI returns.
extern "C" char *morrow_hmos_import(const char *, int owned_fd);
extern "C" char *morrow_hmos_export(const char *, int owned_fd);
extern "C" char *morrow_hmos_prepare(int source_owned_fd, int destination_owned_fd, uint64_t max_bytes);
extern "C" char *morrow_hmos_clipboard_convert(const char *, int source_owned_fd);
extern "C" char *morrow_hmos_clipboard_image(const char *, int source_owned_fd, int destination_owned_fd, uint64_t max_bytes);
extern "C" void morrow_hmos_free(char *);

namespace {
constexpr size_t MAX_REQUEST = 512 * 1024;
constexpr uint64_t MAX_FILE_BYTES = 200ULL * 1024 * 1024;
bool Read(napi_env env, napi_value value, std::string &result) {
    size_t length = 0;
    if (napi_get_value_string_utf8(env, value, nullptr, 0, &length) != napi_ok || length > MAX_REQUEST) return false;
    std::vector<char> bytes(length + 1);
    if (napi_get_value_string_utf8(env, value, bytes.data(), bytes.size(), &length) != napi_ok) return false;
    result.assign(bytes.data(), length);
    return result.find('\0') == std::string::npos;
}
// NAPI UTF8 conversion may replace a lone UTF16 surrogate. Reject it first,
// retaining the caller's original ArkTS value for IME completion/recovery.
bool ReadField(napi_env env, napi_value value, std::string &result) {
    size_t length = 0;
    if (napi_get_value_string_utf16(env, value, nullptr, 0, &length) != napi_ok || length > MAX_REQUEST) return false;
    std::vector<char16_t> units(length + 1);
    size_t copied = 0;
    if (napi_get_value_string_utf16(env, value, units.data(), units.size(), &copied) != napi_ok || copied != length) return false;
    for (size_t i = 0; i < length; ++i) {
        const auto unit = units[i];
        if (unit >= 0xD800 && unit <= 0xDBFF) {
            if (i + 1 >= length || units[i + 1] < 0xDC00 || units[i + 1] > 0xDFFF) return false;
            ++i;
        } else if (unit >= 0xDC00 && unit <= 0xDFFF) { return false; }
    }
    return Read(env, value, result);
}
napi_value Undefined(napi_env env) { napi_value v; napi_get_undefined(env, &v); return v; }
enum class Operation { Request, EditorField, Import, Export, Prepare, ClipboardConvert, ClipboardImage };
struct Work {
    napi_async_work work{};
    napi_deferred deferred{};
    std::string request, reply;
    Operation operation = Operation::Request;
    int source_fd = -1, destination_fd = -1;
    uint64_t max_bytes = 0;
    std::atomic<bool> started{false};
    ~Work() {
        // Never retry close after EINTR: the descriptor may already have been
        // released and reused. A transferred descriptor is set to -1 first.
        if (source_fd >= 0) close(source_fd);
        if (destination_fd >= 0) close(destination_fd);
    }
};
bool ReadFd(napi_env env, napi_value value, int &fd) {
    double number = -1;
    if (napi_get_value_double(env, value, &number) != napi_ok || !std::isfinite(number) ||
        number < 0 || number > std::numeric_limits<int>::max() || std::floor(number) != number) return false;
    fd = static_cast<int>(number); return true;
}
bool ReadLimit(napi_env env, napi_value value, uint64_t &limit) {
    double number = 0;
    if (napi_get_value_double(env, value, &number) != napi_ok || !std::isfinite(number) ||
        number <= 0 || number > MAX_FILE_BYTES || std::floor(number) != number) return false;
    limit = static_cast<uint64_t>(number); return true;
}
int Duplicate(int fd) {
    int owned;
    do { owned = fcntl(fd, F_DUPFD_CLOEXEC, 0); } while (owned < 0 && errno == EINTR);
    return owned;
}
napi_value Error(napi_env env, const char *code, const char *message) {
    napi_value error, codeValue, text;
    napi_create_string_utf8(env, code, NAPI_AUTO_LENGTH, &codeValue);
    napi_create_string_utf8(env, message, NAPI_AUTO_LENGTH, &text);
    napi_create_error(env, codeValue, text, &error); return error;
}
void Reject(napi_env env, napi_deferred deferred, bool started) {
    napi_reject_deferred(env, deferred, Error(env, started ? "NATIVE_OUTCOME_UNKNOWN" : "NATIVE_NOT_STARTED",
        started ? "Native request outcome unknown; retain the original request" : "Native request was not started"));
}
void Execute(napi_env, void *data) {
    auto *w = static_cast<Work *>(data);
    w->started = true;
    char *reply = nullptr;
    switch (w->operation) {
        case Operation::Request: reply = morrow_hmos_request(w->request.c_str()); break;
        case Operation::EditorField: reply = morrow_hmos_editor_field(w->request.c_str()); break;
        case Operation::Import: reply = morrow_hmos_import(w->request.c_str(), std::exchange(w->source_fd, -1)); break;
        case Operation::Export: reply = morrow_hmos_export(w->request.c_str(), std::exchange(w->source_fd, -1)); break;
        case Operation::ClipboardConvert: reply = morrow_hmos_clipboard_convert(w->request.c_str(), std::exchange(w->source_fd, -1)); break;
        case Operation::ClipboardImage: {
            const int source = std::exchange(w->source_fd, -1);
            const int destination = std::exchange(w->destination_fd, -1);
            reply = morrow_hmos_clipboard_image(w->request.c_str(), source, destination, w->max_bytes); break;
        }
        case Operation::Prepare: {
            const int source = std::exchange(w->source_fd, -1);
            const int destination = std::exchange(w->destination_fd, -1);
            reply = morrow_hmos_prepare(source, destination, w->max_bytes); break;
        }
    }
    if (reply) { w->reply = reply; morrow_hmos_free(reply); }
}
void Complete(napi_env env, napi_status status, void *data) {
    std::unique_ptr<Work> w(static_cast<Work *>(data));
    napi_value value;
    if (status == napi_ok && !w->reply.empty() &&
        napi_create_string_utf8(env, w->reply.c_str(), w->reply.size(), &value) == napi_ok) {
        napi_resolve_deferred(env, w->deferred, value);
    } else {
        Reject(env, w->deferred, w->started.load());
    }
    napi_delete_async_work(env, w->work);
}
napi_value Queue(napi_env env, std::unique_ptr<Work> work) {
    napi_value promise, name;
    if (napi_create_promise(env, &work->deferred, &promise) != napi_ok) {
        napi_throw_error(env, "NATIVE_NOT_STARTED", "Cannot allocate native promise"); return nullptr;
    }
    if (napi_create_string_utf8(env, "Morrow Rust", NAPI_AUTO_LENGTH, &name) != napi_ok) {
        Reject(env, work->deferred, false); return promise;
    }
    if (napi_create_async_work(env, nullptr, name, Execute, Complete, work.get(), &work->work) != napi_ok) {
        Reject(env, work->deferred, false); return promise;
    }
    if (napi_queue_async_work(env, work->work) != napi_ok) {
        napi_delete_async_work(env, work->work); Reject(env, work->deferred, false); return promise;
    }
    work.release(); return promise;
}
napi_value Request(napi_env env, napi_callback_info info) {
    size_t argc = 2; napi_value argv[2];
    if (napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr) != napi_ok) {
        napi_throw_type_error(env, "NATIVE_NOT_STARTED", "Cannot read native request arguments"); return nullptr;
    }
    auto work = std::make_unique<Work>();
    if (argc != 1 || !Read(env, argv[0], work->request)) {
        napi_throw_type_error(env, "NATIVE_NOT_STARTED", "Expected bounded UTF-8 request"); return nullptr;
    }
    return Queue(env, std::move(work));
}
napi_value EditorField(napi_env env, napi_callback_info info) {
    size_t argc = 2; napi_value argv[2];
    if (napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr) != napi_ok) {
        napi_throw_type_error(env, "NATIVE_NOT_STARTED", "Cannot read editor field arguments"); return nullptr;
    }
    auto work = std::make_unique<Work>(); work->operation = Operation::EditorField;
    if (argc != 1 || !ReadField(env, argv[0], work->request)) {
        napi_throw_type_error(env, "NATIVE_NOT_STARTED", "Expected bounded scalar-valid editor field JSON"); return nullptr;
    }
    return Queue(env, std::move(work));
}
napi_value Transfer(napi_env env, napi_callback_info info, Operation operation) {
    size_t argc = 3; napi_value argv[3]; int fd;
    if (napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr) != napi_ok) {
        napi_throw_type_error(env, "NATIVE_NOT_STARTED", "Cannot read file transfer arguments"); return nullptr;
    }
    auto work = std::make_unique<Work>(); work->operation = operation;
    if (argc != 2 || !Read(env, argv[0], work->request) || !ReadFd(env, argv[1], fd)) {
        napi_throw_type_error(env, "NATIVE_NOT_STARTED", "Expected bounded request and authorized file descriptor"); return nullptr;
    }
    work->source_fd = Duplicate(fd);
    if (work->source_fd < 0) {
        napi_throw_error(env, "NATIVE_NOT_STARTED", "Cannot duplicate authorized file descriptor"); return nullptr;
    }
    return Queue(env, std::move(work));
}
napi_value ImportFile(napi_env env, napi_callback_info info) { return Transfer(env, info, Operation::Import); }
napi_value ExportFile(napi_env env, napi_callback_info info) { return Transfer(env, info, Operation::Export); }
napi_value ClipboardConvert(napi_env env, napi_callback_info info) { return Transfer(env, info, Operation::ClipboardConvert); }
napi_value ClipboardImage(napi_env env, napi_callback_info info) {
    size_t argc = 5; napi_value argv[5]; int source, destination;
    if (napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr) != napi_ok) {
        napi_throw_type_error(env, "NATIVE_NOT_STARTED", "Cannot read clipboard image arguments"); return nullptr;
    }
    auto work = std::make_unique<Work>(); work->operation = Operation::ClipboardImage;
    if (argc != 4 || !Read(env, argv[0], work->request) || !ReadFd(env, argv[1], source) ||
        !ReadFd(env, argv[2], destination) || source == destination || !ReadLimit(env, argv[3], work->max_bytes)) {
        napi_throw_type_error(env, "NATIVE_NOT_STARTED", "Expected bounded clipboard request, distinct authorized FDs and byte limit"); return nullptr;
    }
    work->source_fd = Duplicate(source);
    if (work->source_fd >= 0) work->destination_fd = Duplicate(destination);
    if (work->source_fd < 0 || work->destination_fd < 0) {
        napi_throw_error(env, "NATIVE_NOT_STARTED", "Cannot duplicate clipboard file descriptors"); return nullptr;
    }
    return Queue(env, std::move(work));
}
napi_value PrepareFile(napi_env env, napi_callback_info info) {
    size_t argc = 4; napi_value argv[4]; int source, destination;
    if (napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr) != napi_ok) {
        napi_throw_type_error(env, "NATIVE_NOT_STARTED", "Cannot read file preparation arguments"); return nullptr;
    }
    auto work = std::make_unique<Work>(); work->operation = Operation::Prepare;
    if (argc != 3 || !ReadFd(env, argv[0], source) || !ReadFd(env, argv[1], destination) || source == destination ||
        !ReadLimit(env, argv[2], work->max_bytes)) {
        napi_throw_type_error(env, "NATIVE_NOT_STARTED", "Expected distinct file descriptors and a limit of 1..200 MiB"); return nullptr;
    }
    work->source_fd = Duplicate(source);
    if (work->source_fd >= 0) work->destination_fd = Duplicate(destination);
    if (work->source_fd < 0 || work->destination_fd < 0) {
        napi_throw_error(env, "NATIVE_NOT_STARTED", "Cannot duplicate authorized file descriptors"); return nullptr;
    }
    return Queue(env, std::move(work));
}

ArkUI_NativeNodeAPI_1 *api = nullptr;
struct Preview { ArkUI_NodeHandle root{}, title{}, detail{}; };
std::map<ArkUI_NodeContentHandle, Preview> previews;
void Dispose(ArkUI_NodeContentHandle content) {
    auto found = previews.find(content);
    if (found == previews.end()) return;
    auto p = found->second;
    OH_ArkUI_NodeContent_RemoveNode(content, p.root);
    api->removeChild(p.root, p.title); api->removeChild(p.root, p.detail);
    api->disposeNode(p.title); api->disposeNode(p.detail); api->disposeNode(p.root);
    previews.erase(found);
}
void Text(ArkUI_NodeHandle node, const std::string &text) {
    ArkUI_AttributeItem item{}; item.string = text.c_str(); api->setAttribute(node, NODE_TEXT_CONTENT, &item);
}
void Number(ArkUI_NodeHandle node, ArkUI_NodeAttributeType key, float n) {
    ArkUI_NumberValue value{}; value.f32 = n; ArkUI_AttributeItem item{&value, 1}; api->setAttribute(node, key, &item);
}
napi_value Render(napi_env env, napi_callback_info info) {
    size_t argc = 4; napi_value args[4]; napi_get_cb_info(env, info, &argc, args, nullptr, nullptr);
    ArkUI_NodeContentHandle content = nullptr; std::string title, detail;
    if (argc != 4 || OH_ArkUI_GetNodeContentFromNapiValue(env, args[0], &content) != 0 ||
        !Read(env, args[1], title) || !Read(env, args[2], detail)) {
        napi_throw_type_error(env, nullptr, "Invalid native preview"); return nullptr;
    }
    if (!api) OH_ArkUI_GetModuleInterface(ARKUI_NATIVE_NODE, ArkUI_NativeNodeAPI_1, api);
    if (!api) { napi_throw_error(env, nullptr, "ArkUI NDK unavailable"); return nullptr; }
    auto found = previews.find(content);
    if (found == previews.end()) {
        Preview p{api->createNode(ARKUI_NODE_COLUMN), api->createNode(ARKUI_NODE_TEXT), api->createNode(ARKUI_NODE_TEXT)};
        if (!p.root || !p.title || !p.detail) {
            if(p.title) api->disposeNode(p.title); if(p.detail) api->disposeNode(p.detail); if(p.root) api->disposeNode(p.root);
            napi_throw_error(env, nullptr, "Native allocation failed"); return nullptr;
        }
        Number(p.root, NODE_WIDTH_PERCENT, 1.0f); Number(p.root, NODE_PADDING, 8);
        Number(p.title, NODE_FONT_SIZE, 12); Number(p.detail, NODE_FONT_SIZE, 10);
        ArkUI_NumberValue gap[4]{}; gap[0].f32 = 0; ArkUI_AttributeItem gapItem{gap, 4}; api->setAttribute(p.detail, NODE_MARGIN, &gapItem);
        api->addChild(p.root, p.title); api->addChild(p.root, p.detail);
        previews.emplace(content, p);
        if (OH_ArkUI_NodeContent_AddNode(content, p.root) != 0) {
            Dispose(content); napi_throw_error(env, nullptr, "Native mount failed"); return nullptr;
        }
        found = previews.find(content);
    }
    bool dark = false; napi_get_value_bool(env, args[3], &dark);
    ArkUI_NumberValue ink{}; ink.u32 = dark ? 0xFFF0EDF8 : 0xFF302D43;
    ArkUI_AttributeItem inkItem{&ink, 1}; api->setAttribute(found->second.title, NODE_FONT_COLOR, &inkItem);
    ArkUI_NumberValue muted{}; muted.u32 = dark ? 0xFFB4AEC5 : 0xFF777184;
    ArkUI_AttributeItem mutedItem{&muted, 1}; api->setAttribute(found->second.detail, NODE_FONT_COLOR, &mutedItem);
    Text(found->second.title, title); Text(found->second.detail, detail);
    return Undefined(env);
}
napi_value Release(napi_env env, napi_callback_info info) {
    size_t argc=1; napi_value args[1]; napi_get_cb_info(env,info,&argc,args,nullptr,nullptr);
    ArkUI_NodeContentHandle content=nullptr;
    if(argc==1 && OH_ArkUI_GetNodeContentFromNapiValue(env,args[0],&content)==0) Dispose(content);
    return Undefined(env);
}
napi_value Init(napi_env env, napi_value exports) {
    napi_property_descriptor methods[] = {
        {"request",nullptr,Request,nullptr,nullptr,nullptr,napi_default,nullptr},
        {"editorField",nullptr,EditorField,nullptr,nullptr,nullptr,napi_default,nullptr},
        {"importFile",nullptr,ImportFile,nullptr,nullptr,nullptr,napi_default,nullptr},
        {"exportFile",nullptr,ExportFile,nullptr,nullptr,nullptr,napi_default,nullptr},
        {"prepareFile",nullptr,PrepareFile,nullptr,nullptr,nullptr,napi_default,nullptr},
        {"clipboardConvert",nullptr,ClipboardConvert,nullptr,nullptr,nullptr,napi_default,nullptr},
        {"clipboardImage",nullptr,ClipboardImage,nullptr,nullptr,nullptr,napi_default,nullptr},
        {"renderPreview",nullptr,Render,nullptr,nullptr,nullptr,napi_default,nullptr},
        {"releasePreview",nullptr,Release,nullptr,nullptr,nullptr,napi_default,nullptr}
    };
    napi_define_properties(env,exports,sizeof(methods) / sizeof(methods[0]),methods); return exports;
}
napi_module module = {1,0,nullptr,Init,"morrow",nullptr,{0}};
__attribute__((constructor)) void Register() { napi_module_register(&module); }
}


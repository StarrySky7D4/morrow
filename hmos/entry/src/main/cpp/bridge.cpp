#include <napi/native_api.h>
#include <arkui/native_interface.h>
#include <arkui/native_node.h>
#include <arkui/native_node_napi.h>
#include <map>
#include <memory>
#include <string>
#include <vector>

extern "C" char *morrow_hmos_request(const char *);
extern "C" void morrow_hmos_free(char *);

namespace {
constexpr size_t MAX_REQUEST = 512 * 1024;
bool Read(napi_env env, napi_value value, std::string &result) {
    size_t length = 0;
    if (napi_get_value_string_utf8(env, value, nullptr, 0, &length) != napi_ok || length > MAX_REQUEST) return false;
    std::vector<char> bytes(length + 1);
    if (napi_get_value_string_utf8(env, value, bytes.data(), bytes.size(), &length) != napi_ok) return false;
    result.assign(bytes.data(), length);
    return result.find('\0') == std::string::npos;
}
napi_value Undefined(napi_env env) { napi_value v; napi_get_undefined(env, &v); return v; }
struct Work { napi_async_work work{}; napi_deferred deferred{}; std::string request, reply; };
void Execute(napi_env, void *data) {
    auto *w = static_cast<Work *>(data);
    char *reply = morrow_hmos_request(w->request.c_str());
    if (reply) { w->reply = reply; morrow_hmos_free(reply); }
}
void Complete(napi_env env, napi_status status, void *data) {
    std::unique_ptr<Work> w(static_cast<Work *>(data));
    napi_value value;
    if (status == napi_ok && !w->reply.empty()) {
        napi_create_string_utf8(env, w->reply.c_str(), w->reply.size(), &value);
        napi_resolve_deferred(env, w->deferred, value);
    } else {
        napi_value message; napi_create_string_utf8(env, "Native request outcome unknown", NAPI_AUTO_LENGTH, &message);
        napi_create_error(env, nullptr, message, &value); napi_reject_deferred(env, w->deferred, value);
    }
    napi_delete_async_work(env, w->work);
}
napi_value Request(napi_env env, napi_callback_info info) {
    size_t argc = 1; napi_value argv[1];
    napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr);
    auto work = std::make_unique<Work>();
    if (argc != 1 || !Read(env, argv[0], work->request)) {
        napi_throw_type_error(env, nullptr, "Expected bounded UTF-8 request"); return nullptr;
    }
    napi_value promise, name;
    napi_create_promise(env, &work->deferred, &promise);
    napi_create_string_utf8(env, "Morrow Rust", NAPI_AUTO_LENGTH, &name);
    if (napi_create_async_work(env, nullptr, name, Execute, Complete, work.get(), &work->work) != napi_ok) {
        napi_reject_deferred(env, work->deferred, name); return promise;
    }
    if (napi_queue_async_work(env, work->work) != napi_ok) {
        napi_delete_async_work(env, work->work); napi_reject_deferred(env, work->deferred, name); return promise;
    }
    work.release(); return promise;
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
        {"renderPreview",nullptr,Render,nullptr,nullptr,nullptr,napi_default,nullptr},
        {"releasePreview",nullptr,Release,nullptr,nullptr,nullptr,napi_default,nullptr}
    };
    napi_define_properties(env,exports,3,methods); return exports;
}
napi_module module = {1,0,nullptr,Init,"morrow",nullptr,{0}};
__attribute__((constructor)) void Register() { napi_module_register(&module); }
}


// Private synchronous bridge. The original directory worker owns this entire
// STA lifetime; no File, COM pointer, window or callback crosses to another owner.
#include <windows.h>
#include <shobjidl.h>
#include <new>
#include <cstdint>

using Callback = int (__cdecl *)(void*, unsigned, const std::uint16_t*, std::size_t);
// Events: 0 = original-authority check, 1 = capture before OnFileOk accepts,
// 2 = native retirement is unknown. Return 0 only on a successful Rust callback.
struct Picker;
static thread_local Picker* active_picker = nullptr;

struct Picker {
    Callback callback;
    void* context;
    IFileOpenDialog* dialog = nullptr;
    DWORD thread = GetCurrentThreadId();
    bool busy = false;
    bool attempted = false;
    bool accepted = false;
    bool native_failure = false;
    bool close_attempted = false;
    bool opened = false;
    HWND dialog_window = nullptr; // borrowed only while the original Show is live
    UINT_PTR timer = 0;

    [[noreturn]] void retain_unknown() {
        // Publish once, then retain the original worker, its callback stack,
        // dialog and all original directory resources. No cleanup/retry is
        // inferred from a failed Unadvise, KillTimer or queued close request.
        callback(context, 2, nullptr, 0);
        for (;;) Sleep(INFINITE);
    }
    int call(unsigned event, const std::uint16_t* path = nullptr, std::size_t count = 0) {
        if (GetCurrentThreadId() != thread || busy) return 1;
        busy = true;
        const int result = callback(context, event, path, count);
        busy = false;
        return result;
    }
    void stop_timer() {
        if (!timer) return;
        if (GetCurrentThreadId() != thread || !KillTimer(nullptr, timer)) retain_unknown();
        timer = 0;
    }
    void cancel() {
        if (close_attempted) return;
        close_attempted = true;
        // The original Show owns this window on this thread. Retire only our
        // timer, then request its ordinary close once. Posting is not evidence
        // of closure: the original callback stack remains charged until Show
        // actually returns and the original COM references retire below.
        if (GetCurrentThreadId() != thread || !dialog_window ||
            GetWindowThreadProcessId(dialog_window, nullptr) != thread) retain_unknown();
        stop_timer();
        if (!PostMessageW(dialog_window, WM_CLOSE, 0, 0)) retain_unknown();
    }
};

class Events final : public IFileDialogEvents {
    LONG references = 1;
    Picker& picker;
public:
    explicit Events(Picker& value) : picker(value) {}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id, void** output) override {
        if (!output) return E_POINTER;
        *output = nullptr;
        if (id == IID_IUnknown || id == IID_IFileDialogEvents) {
            *output = static_cast<IFileDialogEvents*>(this);
            AddRef();
            return S_OK;
        }
        return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override {
        return static_cast<ULONG>(InterlockedIncrement(&references));
    }
    ULONG STDMETHODCALLTYPE Release() override {
        const LONG left = InterlockedDecrement(&references);
        if (left == 0) delete this;
        return static_cast<ULONG>(left);
    }
    HRESULT STDMETHODCALLTYPE OnFileOk(IFileDialog* source) override {
        // One confirmation only. A rejected capture is never replaced by a
        // second pathname, entropy source, approval or attempt on this request.
        if (picker.attempted || picker.close_attempted || picker.busy) return S_FALSE;
        picker.attempted = true;
        if (picker.call(0) != 0) {
            picker.cancel();
            return S_FALSE;
        }
        IShellItem* item = nullptr;
        PWSTR path = nullptr;
        HRESULT result = source->GetResult(&item);
        if (result == S_OK && item) result = item->GetDisplayName(SIGDN_FILESYSPATH, &path);
        if (result == S_OK && path) {
            std::size_t count = 0;
            while (count <= 4096 && path[count] != 0) ++count;
            if (count != 0 && count <= 4096) {
                static_assert(sizeof(wchar_t) == sizeof(std::uint16_t));
                picker.accepted = picker.call(1, reinterpret_cast<const std::uint16_t*>(path), count) == 0;
            } else picker.native_failure = true;
        } else picker.native_failure = true;
        if (path) CoTaskMemFree(path);
        if (item) item->Release();
        if (!picker.accepted) {
            picker.cancel();
            return S_FALSE;
        }
        // Actual original directory and ancestors have already been captured.
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE OnFolderChanging(IFileDialog*, IShellItem*) override { return S_OK; }
    HRESULT STDMETHODCALLTYPE OnFolderChange(IFileDialog* source) override {
        if (!picker.opened) {
            // The initial folder callback can precede visibility. Borrow the
            // actual dialog window once; never find another window by title.
            IOleWindow* window = nullptr;
            HRESULT result = source->QueryInterface(IID_PPV_ARGS(&window));
            if (result != S_OK || !window) picker.retain_unknown();
            result = window->GetWindow(&picker.dialog_window);
            window->Release();
            if (result != S_OK || !picker.dialog_window ||
                GetWindowThreadProcessId(picker.dialog_window, nullptr) != picker.thread)
                picker.retain_unknown();
            picker.opened = true;
        }
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE OnSelectionChange(IFileDialog*) override { return S_OK; }
    HRESULT STDMETHODCALLTYPE OnShareViolation(IFileDialog*, IShellItem*, FDE_SHAREVIOLATION_RESPONSE* response) override {
        if (!response) return E_POINTER;
        *response = FDESVR_DEFAULT;
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE OnTypeChange(IFileDialog*) override { return S_OK; }
    HRESULT STDMETHODCALLTYPE OnOverwrite(IFileDialog*, IShellItem*, FDE_OVERWRITE_RESPONSE* response) override {
        if (!response) return E_POINTER;
        *response = FDEOR_DEFAULT;
        return S_OK;
    }
};

static void CALLBACK tick(HWND, UINT, UINT_PTR timer, DWORD) {
    Picker* picker = active_picker;
    // Reentrant modal messages cannot alias the mutable Rust callback.
    // KillTimer does not remove already queued messages. A stale tick cannot
    // enter the callback after the original timer has been retired.
    if (picker && timer == picker->timer && picker->timer && !picker->busy &&
        !picker->close_attempted && picker->call(0) != 0 && picker->opened &&
        IsWindowVisible(picker->dialog_window)) picker->cancel();
}

// Result: 0 accepted, 1 cancelled, 2 unavailable/native failure. Unknown native
// retirement deliberately does not return across the borrowed Rust callback.
extern "C" int __cdecl morrow_directory_picker(Callback callback, void* context) {
    if (!callback || !context || active_picker) return 2;
    Picker picker{callback, context};
    if (picker.call(0) != 0) return 1;
    const HRESULT initialized = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    if (initialized != S_OK && initialized != S_FALSE) return 2;
    HRESULT result = CoCreateInstance(CLSID_FileOpenDialog, nullptr, CLSCTX_INPROC_SERVER,
        IID_PPV_ARGS(&picker.dialog));
    if (result != S_OK || !picker.dialog) {
        if (picker.dialog) picker.dialog->Release();
        CoUninitialize();
        return 2;
    }
    Events* events = nullptr;
    DWORD cookie = 0;
    bool advised = false;
    FILEOPENDIALOGOPTIONS options = 0;
    result = picker.dialog->GetOptions(&options);
    if (result == S_OK) result = picker.dialog->SetOptions(
        (options & ~(FOS_ALLOWMULTISELECT | FOS_ALLNONSTORAGEITEMS)) |
        FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST |
        FOS_DONTADDTORECENT | FOS_NOCHANGEDIR | FOS_NODEREFERENCELINKS);
    if (result == S_OK) result = picker.dialog->SetTitle(L"Morrow - select directory");
    if (result == S_OK) {
        events = new (std::nothrow) Events(picker);
        if (!events) result = E_OUTOFMEMORY;
    }
    if (result == S_OK) {
        result = picker.dialog->Advise(events, &cookie);
        advised = result == S_OK;
    }
    if (result == S_OK) {
        active_picker = &picker;
        picker.timer = SetTimer(nullptr, 0, 50, tick);
        if (!picker.timer) result = E_FAIL;
    }
    if (result == S_OK) result = picker.dialog->Show(nullptr);
    picker.stop_timer();
    picker.dialog_window = nullptr;
    active_picker = nullptr;
    if (advised && picker.dialog->Unadvise(cookie) != S_OK) picker.retain_unknown();
    picker.dialog->Release();
    picker.dialog = nullptr;
    // The only remaining reference must be our original reference. Retain the
    // original callback stack if any unexpected holder remains after Unadvise.
    if (events && events->Release() != 0) picker.retain_unknown();
    CoUninitialize();
    if (result == S_OK && picker.accepted && !picker.close_attempted) return 0;
    return result == HRESULT_FROM_WIN32(ERROR_CANCELLED) && !picker.native_failure ? 1 : 2;
}

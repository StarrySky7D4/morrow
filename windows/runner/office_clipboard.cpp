#include "office_clipboard.h"
#include <windows.h>
#include <ole2.h>
#include <gdiplus.h>
#include <wrl/client.h>
#include <flutter/standard_method_codec.h>
#include <algorithm>
#include <cmath>
#include <cstring>
#include <mutex>
#include <optional>
#include <thread>
#include <vector>

using flutter::EncodableValue;
using flutter::EncodableMap;
using flutter::EncodableList;
using Microsoft::WRL::ComPtr;
namespace {
constexpr size_t kMaxBytes = 64 * 1024 * 1024;
constexpr UINT kReady = WM_APP + 146;
using Bytes = std::vector<uint8_t>;
void AddFile(EncodableList& files, const std::string& name, Bytes bytes) {
  if (bytes.empty() || bytes.size() > kMaxBytes) return;
  files.emplace_back(EncodableMap{{EncodableValue("name"), EncodableValue(name)},
                                  {EncodableValue("bytes"), EncodableValue(std::move(bytes))}});
}
Bytes GlobalBytes(HGLOBAL handle, size_t limit = kMaxBytes) {
  if (!handle) return {};
  const size_t size = GlobalSize(handle);
  if (size == 0 || size > limit) return {};
  const auto* ptr = static_cast<const uint8_t*>(GlobalLock(handle));
  if (!ptr) return {};
  Bytes bytes(ptr, ptr + size);
  GlobalUnlock(handle);
  return bytes;
}
std::string Utf8(const wchar_t* text) {
  const int length = static_cast<int>(wcslen(text));
  const int size = WideCharToMultiByte(CP_UTF8, 0, text, length, nullptr, 0, nullptr, nullptr);
  std::string result(static_cast<size_t>(size), '\0');
  if (size) WideCharToMultiByte(CP_UTF8, 0, text, length, result.data(), size, nullptr, nullptr);
  return result;
}
Bytes StreamBytes(IStream* stream) {
  STATSTG stat{};
  if (FAILED(stream->Stat(&stat, STATFLAG_NONAME)) || stat.cbSize.QuadPart > kMaxBytes) return {};
  LARGE_INTEGER zero{};
  if (FAILED(stream->Seek(zero, STREAM_SEEK_SET, nullptr))) return {};
  Bytes bytes(static_cast<size_t>(stat.cbSize.QuadPart));
  ULONG read = 0;
  if (FAILED(stream->Read(bytes.data(), static_cast<ULONG>(bytes.size()), &read))) return {};
  bytes.resize(read);
  return bytes;
}
bool StorageFits(IStorage* storage, size_t& total, int depth = 0) {
  if (depth > 8) return false;
  ComPtr<IEnumSTATSTG> entries;
  if (FAILED(storage->EnumElements(0, nullptr, 0, &entries))) return false;
  STATSTG stat{};
  ULONG fetched = 0;
  int count = 0;
  while (entries->Next(1, &stat, &fetched) == S_OK) {
    bool valid = ++count <= 2048;
    if (stat.type == STGTY_STREAM) {
      if (stat.cbSize.QuadPart > kMaxBytes - total) valid = false;
      else total += static_cast<size_t>(stat.cbSize.QuadPart);
    } else if (stat.type == STGTY_STORAGE) {
      ComPtr<IStorage> child;
      valid = valid && SUCCEEDED(storage->OpenStorage(stat.pwcsName, nullptr,
          STGM_READ | STGM_SHARE_EXCLUSIVE, nullptr, 0, &child)) && StorageFits(child.Get(), total, depth + 1);
    }
    CoTaskMemFree(stat.pwcsName);
    if (!valid) return false;
  }
  return true;
}
bool HasStream(IStorage* storage, const wchar_t* name) {
  ComPtr<IStream> stream;
  return SUCCEEDED(storage->OpenStream(name, nullptr, STGM_READ | STGM_SHARE_EXCLUSIVE, 0, &stream));
}
bool Contains(const Bytes& bytes, const std::string& value) {
  return std::search(bytes.begin(), bytes.end(), value.begin(), value.end()) != bytes.end();
}
void ReadEmbeddedObject(EncodableList& files, EncodableList& warnings) {
  ComPtr<IDataObject> object;
  if (FAILED(OleGetClipboard(&object))) return;
  for (const auto* name : {L"Embedded Object", L"Embed Source"}) {
    FORMATETC format{};
    format.cfFormat = static_cast<CLIPFORMAT>(RegisterClipboardFormatW(name));
    format.dwAspect = DVASPECT_CONTENT;
    format.lindex = -1;
    format.tymed = TYMED_ISTORAGE | TYMED_ISTREAM | TYMED_HGLOBAL;
    if (object->QueryGetData(&format) != S_OK) continue;
    STGMEDIUM medium{};
    if (FAILED(object->GetData(&format, &medium))) continue;
    Bytes bytes;
    std::string filename = "Office-embedded-object.ole";
    if (medium.tymed == TYMED_ISTORAGE && medium.pstg) {
      size_t total = 0;
      if (StorageFits(medium.pstg, total)) {
        ComPtr<IStream> package;
        if (SUCCEEDED(medium.pstg->OpenStream(L"Package", nullptr,
            STGM_READ | STGM_SHARE_EXCLUSIVE, 0, &package))) {
          bytes = StreamBytes(package.Get());
          filename = Contains(bytes, "word/document.xml") ? "Word-selection.docx" :
              Contains(bytes, "xl/workbook.xml") ? "Excel-selection.xlsx" :
              Contains(bytes, "ppt/presentation.xml") ? "PowerPoint-selection.pptx" : "Office-package.zip";
        } else {
          ComPtr<ILockBytes> lock;
          ComPtr<IStorage> copy;
          if (SUCCEEDED(CreateILockBytesOnHGlobal(nullptr, TRUE, &lock)) &&
              SUCCEEDED(StgCreateDocfileOnILockBytes(lock.Get(), STGM_CREATE | STGM_READWRITE | STGM_SHARE_EXCLUSIVE, 0, &copy)) &&
              SUCCEEDED(medium.pstg->CopyTo(0, nullptr, nullptr, copy.Get())) &&
              SUCCEEDED(copy->Commit(STGC_DEFAULT))) {
            HGLOBAL data = nullptr;
            if (SUCCEEDED(GetHGlobalFromILockBytes(lock.Get(), &data))) bytes = GlobalBytes(data);
            if (HasStream(medium.pstg, L"WordDocument")) filename = "Word-selection.doc";
            else if (HasStream(medium.pstg, L"Workbook") || HasStream(medium.pstg, L"Book")) filename = "Excel-selection.xls";
            else if (HasStream(medium.pstg, L"PowerPoint Document")) filename = "PowerPoint-selection.ppt";
          }
        }
      }
    } else if (medium.tymed == TYMED_ISTREAM && medium.pstm) {
      bytes = StreamBytes(medium.pstm);
    } else if (medium.tymed == TYMED_HGLOBAL) {
      bytes = GlobalBytes(medium.hGlobal);
    }
    ReleaseStgMedium(&medium);
    if (!bytes.empty()) {
      AddFile(files, filename, std::move(bytes));
      warnings.emplace_back("Office 嵌入对象已保留为原始附件；图表、公式和版式可用原软件继续编辑。");
    } else {
      warnings.emplace_back("有一个 Office 对象超过限制或无法导出，请在原软件保存后导入。");
    }
    return;
  }
}
Bytes RenderMetafile(HENHMETAFILE emf) {
  ENHMETAHEADER info{};
  if (!GetEnhMetaFileHeader(emf, sizeof(info), &info)) return {};
  double width = (static_cast<double>(info.rclFrame.right) - info.rclFrame.left) * 96 / 2540;
  double height = (static_cast<double>(info.rclFrame.bottom) - info.rclFrame.top) * 96 / 2540;
  if (width <= 0 || height <= 0) return {};
  const double scale = std::min(2.0, 2048.0 / std::max(width, height));
  const int w = static_cast<int>(std::clamp(std::ceil(width * scale), 1.0, 2048.0));
  const int h = static_cast<int>(std::clamp(std::ceil(height * scale), 1.0, 2048.0));
  Gdiplus::Bitmap bitmap(w, h, PixelFormat32bppARGB);
  Gdiplus::Metafile metafile(emf, FALSE);
  {
    Gdiplus::Graphics graphics(&bitmap);
    graphics.Clear(Gdiplus::Color(255, 255, 255, 255));
    graphics.SetSmoothingMode(Gdiplus::SmoothingModeHighQuality);
    if (graphics.DrawImage(&metafile, Gdiplus::Rect(0, 0, w, h)) != Gdiplus::Ok) return {};
  }
  UINT count = 0, size = 0;
  Gdiplus::GetImageEncodersSize(&count, &size);
  if (!size) return {};
  std::vector<uint8_t> encoders(size);
  auto* codecs = reinterpret_cast<Gdiplus::ImageCodecInfo*>(encoders.data());
  if (Gdiplus::GetImageEncoders(count, size, codecs) != Gdiplus::Ok) return {};
  for (UINT i = 0; i < count; ++i) {
    if (wcscmp(codecs[i].MimeType, L"image/png") != 0) continue;
    ComPtr<IStream> stream;
    if (SUCCEEDED(CreateStreamOnHGlobal(nullptr, TRUE, &stream)) &&
        bitmap.Save(stream.Get(), &codecs[i].Clsid, nullptr) == Gdiplus::Ok) return StreamBytes(stream.Get());
  }
  return {};
}
EncodableValue ReadOffice(int64_t expected) {
  EncodableList files, warnings, formats;
  const HRESULT initialized = OleInitialize(nullptr);
  if (FAILED(initialized)) return EncodableMap{{EncodableValue("warnings"), EncodableValue(EncodableList{EncodableValue("Office 剪贴板暂不可用。")})}};
  HENHMETAFILE emf = nullptr;
  const DWORD sequence = GetClipboardSequenceNumber();
  if (expected >= 0 && sequence != static_cast<DWORD>(expected)) {
    OleUninitialize();
    return EncodableMap{{EncodableValue("warnings"), EncodableValue(EncodableList{EncodableValue("剪贴板已变化，请重新粘贴。")})}};
  }
  bool opened = false;
  for (int retry = 0; retry < 4 && !opened; ++retry) {
    opened = OpenClipboard(nullptr) != FALSE;
    if (!opened) Sleep(15);
  }
  if (opened) {
    UINT format = 0;
    int count = 0;
    while ((format = EnumClipboardFormats(format)) != 0 && ++count <= 128) {
      wchar_t name[256]{};
      if (GetClipboardFormatNameW(format, name, 256) > 0) formats.emplace_back(Utf8(name));
    }
    AddFile(files, "Word-rich-text.rtf", GlobalBytes(
        GetClipboardData(RegisterClipboardFormatW(L"Rich Text Format")), 8 * 1024 * 1024));
    AddFile(files, "Excel-cells.xml", GlobalBytes(
        GetClipboardData(RegisterClipboardFormatW(L"XML Spreadsheet")), 2 * 1024 * 1024));
    if (auto handle = static_cast<HENHMETAFILE>(GetClipboardData(CF_ENHMETAFILE))) {
      const UINT size = GetEnhMetaFileBits(handle, 0, nullptr);
      if (size > 0 && size <= 16 * 1024 * 1024) {
        Bytes bytes(size);
        if (GetEnhMetaFileBits(handle, size, bytes.data()) == size) AddFile(files, "Office-vector.emf", std::move(bytes));
        emf = CopyEnhMetaFileW(handle, nullptr);
      }
    }
    CloseClipboard();
    ReadEmbeddedObject(files, warnings);
  } else {
    warnings.emplace_back("剪贴板正被其他应用占用，Office 对象未读取。");
  }
  if (emf) {
    Gdiplus::GdiplusStartupInput input;
    ULONG_PTR token = 0;
    if (Gdiplus::GdiplusStartup(&token, &input, nullptr) == Gdiplus::Ok) {
      AddFile(files, "Office-preview.png", RenderMetafile(emf));
      Gdiplus::GdiplusShutdown(token);
    }
    DeleteEnhMetaFile(emf);
  }
  OleUninitialize();
  if (GetClipboardSequenceNumber() != sequence) {
    files.clear();
    warnings.emplace_back("读取期间剪贴板发生变化，请重新粘贴。");
  }
  return EncodableMap{{EncodableValue("files"), EncodableValue(std::move(files))},
      {EncodableValue("warnings"), EncodableValue(std::move(warnings))},
      {EncodableValue("formats"), EncodableValue(std::move(formats))}};
}
}  // namespace

#ifndef MORROW_CLIPBOARD_TEST
struct OfficeClipboardState {
  std::mutex mutex;
  HWND window = nullptr;
  bool alive = true;
  std::unique_ptr<flutter::MethodResult<EncodableValue>> reply;
  std::optional<EncodableValue> value;
};

OfficeClipboard::OfficeClipboard(flutter::BinaryMessenger* messenger)
    : state_(std::make_shared<OfficeClipboardState>()) {
  WNDCLASSW wc{};
  wc.hInstance = GetModuleHandle(nullptr);
  wc.lpszClassName = L"MorrowOfficeClipboard";
  wc.lpfnWndProc = [](HWND hwnd, UINT message, WPARAM wparam, LPARAM lparam) -> LRESULT {
    if (message == WM_NCCREATE) {
      auto* create = reinterpret_cast<CREATESTRUCTW*>(lparam);
      SetWindowLongPtr(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(create->lpCreateParams));
    }
    auto* state = reinterpret_cast<OfficeClipboardState*>(GetWindowLongPtr(hwnd, GWLP_USERDATA));
    if (message == kReady && state) {
      std::unique_ptr<flutter::MethodResult<EncodableValue>> reply;
      std::optional<EncodableValue> value;
      {
        std::lock_guard<std::mutex> lock(state->mutex);
        reply = std::move(state->reply);
        value = std::move(state->value);
        state->value.reset();
      }
      if (reply && value) reply->Success(*value);
      return 0;
    }
    return DefWindowProc(hwnd, message, wparam, lparam);
  };
  RegisterClassW(&wc);
  state_->window = CreateWindowExW(0, wc.lpszClassName, L"", 0, 0, 0, 0, 0,
      HWND_MESSAGE, nullptr, wc.hInstance, state_.get());
  channel_ = std::make_unique<flutter::MethodChannel<EncodableValue>>(
      messenger, "morrow/office_clipboard", &flutter::StandardMethodCodec::GetInstance());
  channel_->SetMethodCallHandler([state = state_](const auto& call, auto result) {
    if (call.method_name() == "sequence") {
      result->Success(EncodableValue(static_cast<int64_t>(GetClipboardSequenceNumber())));
      return;
    }
    if (call.method_name() != "read") { result->NotImplemented(); return; }
    int64_t sequence = -1;
    if (const auto* args = std::get_if<EncodableMap>(call.arguments())) {
      auto found = args->find(EncodableValue("sequence"));
      if (found != args->end()) {
        if (const auto* n = std::get_if<int64_t>(&found->second)) sequence = *n;
        if (const auto* n = std::get_if<int32_t>(&found->second)) sequence = *n;
      }
    }
    {
      std::lock_guard<std::mutex> lock(state->mutex);
      if (!state->window || state->reply) { result->Error("clipboard_busy", "Office clipboard is busy."); return; }
      state->reply = std::move(result);
    }
    std::thread([state, sequence] {
      EncodableValue value;
      try { value = ReadOffice(sequence); }
      catch (...) { value = EncodableMap{{EncodableValue("warnings"),
          EncodableValue(EncodableList{EncodableValue("Office 内容读取失败，其他剪贴板内容仍可使用。")})}}; }
      std::lock_guard<std::mutex> lock(state->mutex);
      if (!state->alive) return;
      state->value = std::move(value);
      PostMessage(state->window, kReady, 0, 0);
    }).detach();
  });
}
OfficeClipboard::~OfficeClipboard() {
  channel_->SetMethodCallHandler(nullptr);
  {
    std::lock_guard<std::mutex> lock(state_->mutex);
    state_->alive = false;
    state_->reply.reset();
  }
  if (state_->window) DestroyWindow(state_->window);
}
#endif  // MORROW_CLIPBOARD_TEST

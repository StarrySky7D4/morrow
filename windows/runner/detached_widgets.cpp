#include "detached_widgets.h"

#include <algorithm>
#include <cmath>
#include <dwmapi.h>
#include <flutter/standard_method_codec.h>
#include <windowsx.h>

using std::max;
using std::min;

namespace {

constexpr wchar_t kDetachedClassName[] = L"MORROW_DETACHED_WIDGET_WINDOW";
constexpr int kBaseResizeBorder = 8;
constexpr int kTitleEditId = 4101;
constexpr int kBodyEditId = 4102;
constexpr int kTodosEditId = 4103;

template <typename T>
void SafeRelease(T*& ptr) {
  if (ptr) {
    ptr->Release();
    ptr = nullptr;
  }
}

std::wstring Utf8ToWide(const std::string& utf8) {
  if (utf8.empty()) return {};
  const int len = static_cast<int>(utf8.size());
  const int needed = MultiByteToWideChar(CP_UTF8, 0, utf8.data(), len, nullptr, 0);
  if (needed <= 0) return {};
  std::wstring out(static_cast<size_t>(needed), L'\0');
  MultiByteToWideChar(CP_UTF8, 0, utf8.data(), len, out.data(), needed);
  return out;
}

std::string WideToUtf8(const std::wstring& wide) {
  if (wide.empty()) return {};
  const int len = static_cast<int>(wide.size());
  const int needed = WideCharToMultiByte(CP_UTF8, 0, wide.data(), len, nullptr, 0,
                                         nullptr, nullptr);
  if (needed <= 0) return {};
  std::string out(static_cast<size_t>(needed), '\0');
  WideCharToMultiByte(CP_UTF8, 0, wide.data(), len, out.data(), needed,
                      nullptr, nullptr);
  return out;
}

std::wstring NormalizeToCrLf(const std::wstring& text) {
  std::wstring out;
  out.reserve(text.size() + 16);
  for (size_t i = 0; i < text.size(); ++i) {
    const wchar_t ch = text[i];
    if (ch == L'\r') {
      out.push_back(L'\r');
      out.push_back(L'\n');
      if (i + 1 < text.size() && text[i + 1] == L'\n') ++i;
    } else if (ch == L'\n') {
      out.push_back(L'\r');
      out.push_back(L'\n');
    } else {
      out.push_back(ch);
    }
  }
  return out;
}

std::wstring NormalizeToLf(const std::wstring& text) {
  std::wstring out;
  out.reserve(text.size());
  for (size_t i = 0; i < text.size(); ++i) {
    if (text[i] == L'\r') {
      out.push_back(L'\n');
      if (i + 1 < text.size() && text[i + 1] == L'\n') ++i;
    } else {
      out.push_back(text[i]);
    }
  }
  return out;
}

std::wstring ReadEditText(HWND edit) {
  if (!edit || !IsWindow(edit)) return {};
  const int len = GetWindowTextLengthW(edit);
  if (len <= 0) return {};
  std::wstring buf(static_cast<size_t>(len) + 1, L'\0');
  const int copied = GetWindowTextW(edit, buf.data(), len + 1);
  if (copied <= 0) return {};
  buf.resize(static_cast<size_t>(copied));
  return NormalizeToLf(buf);
}

int WheelDeltaToScrollLines(short delta) {
  if (delta == 0) return 0;
  int lines = static_cast<int>(-std::lround(static_cast<double>(delta) / 40.0));
  if (lines == 0) lines = (delta > 0 ? -1 : 1);
  return lines;
}

LRESULT CALLBACK EditWheelSubclassProc(HWND hwnd, UINT message, WPARAM wparam,
                                       LPARAM lparam) {
  WNDPROC orig =
      reinterpret_cast<WNDPROC>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
  if (message == WM_MOUSEWHEEL) {
    POINT pt{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
    RECT rc{};
    GetWindowRect(hwnd, &rc);
    const bool is_multiline =
        (GetWindowLongPtrW(hwnd, GWL_STYLE) & ES_MULTILINE) != 0;
    if (is_multiline && PtInRect(&rc, pt)) {
      const int lines = WheelDeltaToScrollLines(GET_WHEEL_DELTA_WPARAM(wparam));
      if (lines != 0) {
        SendMessageW(hwnd, EM_LINESCROLL, 0, static_cast<LPARAM>(lines));
      }
      return 0;
    }
    if (HWND parent = GetParent(hwnd)) {
      return SendMessageW(parent, WM_MOUSEWHEEL, wparam, lparam);
    }
    return 0;
  }
  if (message == WM_NCDESTROY) {
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
  }
  return orig ? CallWindowProcW(orig, hwnd, message, wparam, lparam)
              : DefWindowProcW(hwnd, message, wparam, lparam);
}

void SubclassEditForWheel(HWND ctrl) {
  if (!ctrl) return;
  const LONG_PTR prev = SetWindowLongPtrW(
      ctrl, GWLP_WNDPROC, reinterpret_cast<LONG_PTR>(EditWheelSubclassProc));
  SetWindowLongPtrW(ctrl, GWLP_USERDATA, prev);
}

std::string ReadString(const flutter::EncodableMap& map, const char* key,
                       const std::string& fallback = "") {
  const auto it = map.find(flutter::EncodableValue(key));
  if (it == map.end()) return fallback;
  if (const auto* s = std::get_if<std::string>(&it->second)) return *s;
  return fallback;
}

bool HasKey(const flutter::EncodableMap& map, const char* key) {
  const auto it = map.find(flutter::EncodableValue(key));
  return it != map.end() && !it->second.IsNull();
}

bool ReadBool(const flutter::EncodableMap& map, const char* key, bool fallback) {
  const auto it = map.find(flutter::EncodableValue(key));
  if (it == map.end()) return fallback;
  if (const auto* b = std::get_if<bool>(&it->second)) return *b;
  return fallback;
}

double ReadDouble(const flutter::EncodableMap& map, const char* key,
                  double fallback) {
  const auto it = map.find(flutter::EncodableValue(key));
  if (it == map.end()) return fallback;
  if (const auto* d = std::get_if<double>(&it->second)) return *d;
  if (const auto* i = std::get_if<int32_t>(&it->second)) return *i;
  if (const auto* i64 = std::get_if<int64_t>(&it->second)) {
    return static_cast<double>(*i64);
  }
  return fallback;
}

int ReadInt(const flutter::EncodableMap& map, const char* key, int fallback) {
  const auto it = map.find(flutter::EncodableValue(key));
  if (it == map.end()) return fallback;
  if (const auto* i = std::get_if<int32_t>(&it->second)) return *i;
  if (const auto* i64 = std::get_if<int64_t>(&it->second)) {
    return static_cast<int>(*i64);
  }
  if (const auto* d = std::get_if<double>(&it->second)) {
    return static_cast<int>(*d);
  }
  return fallback;
}

uint32_t ReadUint32(const flutter::EncodableMap& map, const char* key,
                    uint32_t fallback) {
  const auto it = map.find(flutter::EncodableValue(key));
  if (it == map.end()) return fallback;
  if (const auto* i = std::get_if<int32_t>(&it->second)) {
    return static_cast<uint32_t>(*i);
  }
  if (const auto* i64 = std::get_if<int64_t>(&it->second)) {
    return static_cast<uint32_t>(*i64);
  }
  return fallback;
}

D2D1_COLOR_F ArgbToD2D(uint32_t argb, float alpha_override = -1.0f) {
  const float a = alpha_override >= 0.0f
                      ? std::clamp(alpha_override, 0.0f, 1.0f)
                      : static_cast<float>((argb >> 24) & 0xFF) / 255.0f;
  return D2D1::ColorF(static_cast<float>((argb >> 16) & 0xFF) / 255.0f,
                      static_cast<float>((argb >> 8) & 0xFF) / 255.0f,
                      static_cast<float>(argb & 0xFF) / 255.0f, a);
}

D2D1_COLOR_F LerpD2D(D2D1_COLOR_F a, D2D1_COLOR_F b, float t,
                     float alpha_override = -1.0f) {
  const float u = std::clamp(t, 0.0f, 1.0f);
  const float alpha = alpha_override >= 0.0f
                          ? std::clamp(alpha_override, 0.0f, 1.0f)
                          : (a.a + (b.a - a.a) * u);
  return D2D1::ColorF(a.r + (b.r - a.r) * u, a.g + (b.g - a.g) * u,
                      a.b + (b.b - a.b) * u, alpha);
}

COLORREF D2DToColorRef(D2D1_COLOR_F c) {
  return RGB(static_cast<BYTE>(std::clamp(c.r * 255.0f, 0.0f, 255.0f)),
             static_cast<BYTE>(std::clamp(c.g * 255.0f, 0.0f, 255.0f)),
             static_cast<BYTE>(std::clamp(c.b * 255.0f, 0.0f, 255.0f)));
}

float ScaleRadius(float base, float corner_radius, float scale) {
  return base * (std::clamp(corner_radius, 0.0f, 32.0f) / 20.0f) * scale;
}

IDWriteTextFormat* CreateTextFormat(IDWriteFactory* factory,
                                    const std::wstring& font_family,
                                    float px_size, DWRITE_FONT_WEIGHT weight,
                                    bool single_line = false,
                                    DWRITE_TEXT_ALIGNMENT align =
                                        DWRITE_TEXT_ALIGNMENT_LEADING,
                                    DWRITE_PARAGRAPH_ALIGNMENT valign =
                                        DWRITE_PARAGRAPH_ALIGNMENT_NEAR) {
  if (!factory) return nullptr;
  IDWriteTextFormat* fmt = nullptr;
  const wchar_t* face =
      font_family.empty() ? L"Microsoft YaHei UI" : font_family.c_str();
  HRESULT hr = factory->CreateTextFormat(
      face, nullptr, weight, DWRITE_FONT_STYLE_NORMAL,
      DWRITE_FONT_STRETCH_NORMAL, max(8.0f, px_size), L"zh-CN", &fmt);
  if (FAILED(hr) || !fmt) {
    hr = factory->CreateTextFormat(
        L"Microsoft YaHei UI", nullptr, weight, DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL, max(8.0f, px_size), L"zh-CN", &fmt);
  }
  if (!fmt) return nullptr;
  fmt->SetTextAlignment(align);
  fmt->SetParagraphAlignment(valign);
  if (single_line) {
    fmt->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
    DWRITE_TRIMMING trim{DWRITE_TRIMMING_GRANULARITY_CHARACTER, 0, 0};
    IDWriteInlineObject* ellipsis = nullptr;
    if (SUCCEEDED(factory->CreateEllipsisTrimmingSign(fmt, &ellipsis)) &&
        ellipsis) {
      fmt->SetTrimming(&trim, ellipsis);
      ellipsis->Release();
    }
  } else {
    fmt->SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
  }
  return fmt;
}

void DrawD2DKindIcon(ID2D1RenderTarget* rt, const std::string& kind,
                     D2D1_RECT_F box, D2D1_COLOR_F accent) {
  ID2D1SolidColorBrush* brush = nullptr;
  if (FAILED(rt->CreateSolidColorBrush(accent, &brush)) || !brush) return;
  const float stroke = max(1.3f, (box.right - box.left) * 0.085f);
  const float cx = (box.left + box.right) * 0.5f;
  const float cy = (box.top + box.bottom) * 0.5f;
  const float r = (box.right - box.left) * 0.30f;

  if (kind == "timer") {
    rt->DrawEllipse(D2D1::Ellipse(D2D1::Point2F(cx, cy), r, r), brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx, cy), D2D1::Point2F(cx, cy - r * 0.55f),
                 brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx, cy),
                 D2D1::Point2F(cx + r * 0.50f, cy + r * 0.12f), brush, stroke);
  } else if (kind == "schedule") {
    rt->DrawRoundedRectangle(
        D2D1::RoundedRect(
            D2D1::RectF(cx - r, cy - r * 0.85f, cx + r, cy + r), 2.5f, 2.5f),
        brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx - r, cy - r * 0.28f),
                 D2D1::Point2F(cx + r, cy - r * 0.28f), brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx - r * 0.45f, cy - r * 1.1f),
                 D2D1::Point2F(cx - r * 0.45f, cy - r * 0.65f), brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx + r * 0.45f, cy - r * 1.1f),
                 D2D1::Point2F(cx + r * 0.45f, cy - r * 0.65f), brush, stroke);
  } else if (kind == "countdown") {
    rt->DrawLine(D2D1::Point2F(cx - r * 0.7f, cy - r),
                 D2D1::Point2F(cx + r * 0.7f, cy - r), brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx - r * 0.7f, cy + r),
                 D2D1::Point2F(cx + r * 0.7f, cy + r), brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx - r * 0.65f, cy - r),
                 D2D1::Point2F(cx + r * 0.65f, cy + r), brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx + r * 0.65f, cy - r),
                 D2D1::Point2F(cx - r * 0.65f, cy + r), brush, stroke);
  } else if (kind == "automation") {
    const D2D1_POINT_2F p0{cx + r * 0.15f, cy - r};
    const D2D1_POINT_2F p1{cx - r * 0.55f, cy + r * 0.1f};
    const D2D1_POINT_2F p2{cx + r * 0.55f, cy - r * 0.1f};
    const D2D1_POINT_2F p3{cx - r * 0.15f, cy + r};
    rt->DrawLine(p0, p1, brush, stroke);
    rt->DrawLine(p1, p2, brush, stroke);
    rt->DrawLine(p2, p3, brush, stroke);
  } else {
    rt->DrawLine(D2D1::Point2F(cx, cy - r), D2D1::Point2F(cx, cy + r), brush,
                 stroke);
    rt->DrawLine(D2D1::Point2F(cx - r, cy), D2D1::Point2F(cx + r, cy), brush,
                 stroke);
    rt->DrawLine(D2D1::Point2F(cx - r * 0.55f, cy - r * 0.55f),
                 D2D1::Point2F(cx + r * 0.55f, cy + r * 0.55f), brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx + r * 0.55f, cy - r * 0.55f),
                 D2D1::Point2F(cx - r * 0.55f, cy + r * 0.55f), brush, stroke);
  }
  brush->Release();
}

void DrawD2DActionIcon(ID2D1RenderTarget* rt, const std::string& action,
                       D2D1_RECT_F box, D2D1_COLOR_F color,
                       bool filled = false) {
  ID2D1SolidColorBrush* brush = nullptr;
  if (FAILED(rt->CreateSolidColorBrush(color, &brush)) || !brush) return;
  const float w = box.right - box.left;
  const float h = box.bottom - box.top;
  const float stroke = max(1.35f, min(w, h) * 0.062f);
  const float cx = (box.left + box.right) * 0.5f;
  const float cy = (box.top + box.bottom) * 0.5f;
  const float r = min(w, h) * 0.25f;

  if (action == "close") {
    rt->DrawLine(D2D1::Point2F(cx - r * 0.72f, cy - r * 0.72f),
                 D2D1::Point2F(cx + r * 0.72f, cy + r * 0.72f), brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx + r * 0.72f, cy - r * 0.72f),
                 D2D1::Point2F(cx - r * 0.72f, cy + r * 0.72f), brush, stroke);
  } else if (action == "pin") {
    const D2D1_RECT_F head =
        D2D1::RectF(cx - r * 0.52f, cy - r * 0.85f, cx + r * 0.52f,
                    cy - r * 0.12f);
    if (filled) {
      rt->FillRoundedRectangle(D2D1::RoundedRect(head, stroke, stroke), brush);
    } else {
      rt->DrawRoundedRectangle(D2D1::RoundedRect(head, stroke, stroke), brush,
                               stroke);
    }
    rt->DrawLine(D2D1::Point2F(cx - r * 0.72f, cy - r * 0.10f),
                 D2D1::Point2F(cx + r * 0.72f, cy - r * 0.10f), brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx, cy - r * 0.10f),
                 D2D1::Point2F(cx, cy + r * 0.85f), brush, stroke);
  } else if (action == "dock") {
    rt->DrawRoundedRectangle(
        D2D1::RoundedRect(
            D2D1::RectF(cx - r * 0.80f, cy - r * 0.75f, cx + r * 0.80f,
                        cy + r * 0.75f),
            stroke * 1.3f, stroke * 1.3f),
        brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx - r * 0.30f, cy),
                 D2D1::Point2F(cx + r * 0.48f, cy), brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx - r * 0.30f, cy),
                 D2D1::Point2F(cx + r * 0.08f, cy - r * 0.35f), brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx - r * 0.30f, cy),
                 D2D1::Point2F(cx + r * 0.08f, cy + r * 0.35f), brush, stroke);
  } else if (action == "edit") {
    rt->DrawLine(D2D1::Point2F(cx - r * 0.55f, cy + r * 0.55f),
                 D2D1::Point2F(cx + r * 0.55f, cy - r * 0.55f), brush,
                 stroke * 1.15f);
    rt->DrawLine(D2D1::Point2F(cx - r * 0.75f, cy + r * 0.75f),
                 D2D1::Point2F(cx - r * 0.25f, cy + r * 0.68f), brush, stroke);
    rt->DrawLine(D2D1::Point2F(cx - r * 0.75f, cy + r * 0.75f),
                 D2D1::Point2F(cx - r * 0.68f, cy + r * 0.25f), brush, stroke);
  } else if (action == "preview") {
    rt->DrawEllipse(D2D1::Ellipse(D2D1::Point2F(cx, cy), r * 0.90f, r * 0.52f),
                    brush, stroke);
    rt->FillEllipse(D2D1::Ellipse(D2D1::Point2F(cx, cy), r * 0.28f, r * 0.28f),
                    brush);
  }
  brush->Release();
}

void DrawResizeGrip(ID2D1RenderTarget* rt, float fw, float fh, float scale,
                    D2D1_COLOR_F muted) {
  ID2D1SolidColorBrush* grip_b = nullptr;
  if (FAILED(rt->CreateSolidColorBrush(
          D2D1::ColorF(muted.r, muted.g, muted.b, 0.38f), &grip_b)) ||
      !grip_b) {
    return;
  }
  const float stroke = max(1.1f, 1.1f * scale);
  rt->DrawLine(D2D1::Point2F(fw - 13.0f * scale, fh - 5.0f * scale),
               D2D1::Point2F(fw - 5.0f * scale, fh - 13.0f * scale), grip_b,
               stroke);
  rt->DrawLine(D2D1::Point2F(fw - 9.0f * scale, fh - 5.0f * scale),
               D2D1::Point2F(fw - 5.0f * scale, fh - 9.0f * scale), grip_b,
               stroke);
  grip_b->Release();
}

std::vector<std::wstring> SplitLines(const std::wstring& text) {
  std::vector<std::wstring> out;
  size_t start = 0;
  while (start <= text.size()) {
    const size_t pos = text.find(L'\n', start);
    if (pos == std::wstring::npos) {
      out.push_back(text.substr(start));
      break;
    }
    out.push_back(text.substr(start, pos - start));
    start = pos + 1;
  }
  return out;
}

}  // namespace

DetachedWidgets::DetachedWidgets(HWND main_window,
                                 flutter::BinaryMessenger* messenger)
    : main_window_(main_window),
      channel_(std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          messenger, "morrow/detached_widgets",
          &flutter::StandardMethodCodec::GetInstance())) {
  EnsureD2DFactories();
  RegisterWindowClass();

  channel_->SetMethodCallHandler(
      [this](const flutter::MethodCall<flutter::EncodableValue>& call,
             std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>>
                 result) {
        if (call.method_name() == "isSupported") {
          result->Success(flutter::EncodableValue(true));
          return;
        }
        const auto* args = call.arguments()
                               ? std::get_if<flutter::EncodableMap>(call.arguments())
                               : nullptr;
        if (call.method_name() == "openOrUpdate") {
          if (!args) {
            result->Error("invalid_args", "Expected a map for openOrUpdate.");
            return;
          }
          OpenOrUpdate(*args);
          result->Success();
          return;
        }
        if (call.method_name() == "openEditor") {
          if (!args) {
            result->Error("invalid_args", "Expected a map for openEditor.");
            return;
          }
          OpenEditor(*args);
          result->Success();
          return;
        }
        if (call.method_name() == "close") {
          if (!args) {
            result->Error("invalid_args", "Expected a map for close.");
            return;
          }
          CloseById(ReadString(*args, "id"));
          result->Success();
          return;
        }
        if (call.method_name() == "closeAll") {
          CloseAll();
          result->Success();
          return;
        }
        result->NotImplemented();
      });
}

DetachedWidgets::~DetachedWidgets() {
  CloseAll();
  channel_ = nullptr;
  SafeRelease(dc_target_);
  SafeRelease(dwrite_factory_);
  SafeRelease(d2d_factory_);
}

bool DetachedWidgets::EnsureD2DFactories() {
  if (!d2d_factory_) {
    D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, &d2d_factory_);
  }
  if (!dwrite_factory_) {
    DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory),
                        reinterpret_cast<IUnknown**>(&dwrite_factory_));
  }
  if (!dc_target_ && d2d_factory_) {
    const D2D1_RENDER_TARGET_PROPERTIES props = D2D1::RenderTargetProperties(
        D2D1_RENDER_TARGET_TYPE_DEFAULT,
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_IGNORE),
        96.0f, 96.0f, D2D1_RENDER_TARGET_USAGE_NONE,
        D2D1_FEATURE_LEVEL_DEFAULT);
    d2d_factory_->CreateDCRenderTarget(&props, &dc_target_);
  }
  return d2d_factory_ != nullptr && dwrite_factory_ != nullptr &&
         dc_target_ != nullptr;
}

void DetachedWidgets::RegisterWindowClass() {
  if (class_registered_) return;
  WNDCLASSEXW wc{};
  wc.cbSize = sizeof(wc);
  wc.style = CS_HREDRAW | CS_VREDRAW;
  wc.lpfnWndProc = DetachedWidgets::WndProc;
  wc.hInstance = GetModuleHandle(nullptr);
  wc.hCursor = LoadCursor(nullptr, IDC_ARROW);
  wc.lpszClassName = kDetachedClassName;
  if (RegisterClassExW(&wc) != 0 || GetLastError() == ERROR_CLASS_ALREADY_EXISTS) {
    class_registered_ = true;
  }
}

void DetachedWidgets::ApplyWindowChrome(DetachedWindow* win) {
  if (!win || !win->hwnd) return;
  const BOOL dark_mode = win->dark ? TRUE : FALSE;
  DwmSetWindowAttribute(win->hwnd, 20 /* DWMWA_USE_IMMERSIVE_DARK_MODE */,
                        &dark_mode, sizeof(dark_mode));
  const DWORD pref =
      win->corner_radius < 2.0f
          ? 1u
          : (win->corner_radius < 10.0f ? 3u : 2u);
  DwmSetWindowAttribute(win->hwnd, 33 /* DWMWA_WINDOW_CORNER_PREFERENCE */,
                        &pref, sizeof(pref));

  const D2D1_COLOR_F surface = ArgbToD2D(win->surface_argb, 1.0f);
  const D2D1_COLOR_F edge = ArgbToD2D(win->edge_argb);
  const COLORREF border_rgb =
      D2DToColorRef(LerpD2D(surface, edge, max(0.32f, edge.a), 1.0f));
  DwmSetWindowAttribute(win->hwnd, 34 /* DWMWA_BORDER_COLOR */, &border_rgb,
                        sizeof(border_rgb));

  const BYTE layer_alpha =
      win->is_solid
          ? static_cast<BYTE>(255)
          : static_cast<BYTE>(
                std::clamp(220.0f + win->frosted_opacity * 30.0f, 224.0f, 250.0f));
  SetLayeredWindowAttributes(win->hwnd, 0, layer_alpha, LWA_ALPHA);
}

void DetachedWidgets::UpdateTopmostFlag(DetachedWindow* win) {
  if (!win || !win->hwnd) return;
  SetWindowPos(win->hwnd, win->pinned ? HWND_TOPMOST : HWND_NOTOPMOST, 0, 0, 0,
               0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
}

void DetachedWidgets::ReadThemeTokens(const flutter::EncodableMap& args,
                                      DetachedWindow* win) {
  win->dark = ReadBool(args, "dark", false);
  const uint32_t def_surface = win->dark ? 0xFF292634 : 0xFFFFFFFF;
  const uint32_t def_bg = win->dark ? 0xFF181720 : 0xFFF9F8FC;
  const uint32_t def_ink = win->dark ? 0xFFF0EDF8 : 0xFF302D43;
  const uint32_t def_muted = win->dark ? 0xFFB4AEC5 : 0xFF777184;
  const uint32_t def_accent = win->dark ? 0xFFC0AFFA : 0xFF7662BA;
  const uint32_t def_on_accent = win->dark ? 0xFF181720 : 0xFFFFFFFF;
  const uint32_t def_line = win->dark ? 0x22F0EDF8 : 0x16302D43;
  const uint32_t def_edge = win->dark ? 0x45FFFFFF : 0x38302D43;

  win->bg_argb = ReadUint32(args, "bgArgb", def_bg);
  win->surface_argb = ReadUint32(args, "surfaceArgb", def_surface);
  win->ink_argb = ReadUint32(args, "inkArgb", def_ink);
  win->muted_argb = ReadUint32(args, "mutedArgb", def_muted);
  win->accent_argb = ReadUint32(args, "accentArgb", def_accent);
  win->on_accent_argb = ReadUint32(args, "onAccentArgb", def_on_accent);
  win->line_argb = ReadUint32(args, "lineArgb", def_line);
  win->edge_argb = ReadUint32(args, "edgeArgb", def_edge);
  win->corner_radius = static_cast<float>(
      std::clamp(ReadDouble(args, "cornerRadius", 20.0), 0.0, 32.0));
  win->frosted_opacity = static_cast<float>(
      std::clamp(ReadDouble(args, "frostedOpacity", 0.88), 0.2, 1.0));
  win->is_solid = ReadBool(args, "isSolid", false);
  const std::string font = ReadString(args, "fontFamily");
  win->font_family = font.empty() ? L"Microsoft YaHei UI" : Utf8ToWide(font);
}

POINT DetachedWidgets::ComputeInitialPosition(const flutter::EncodableMap& args,
                                              int px_w, int px_h) {
  const int vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
  const int vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
  const int vw = max(640, GetSystemMetrics(SM_CXVIRTUALSCREEN));
  const int vh = max(480, GetSystemMetrics(SM_CYVIRTUALSCREEN));
  const int max_vx = vx + vw - px_w - 8;
  const int max_vy = vy + vh - px_h - 8;

  const UINT dpi = main_window_ ? GetDpiForWindow(main_window_) : 96;
  if (HasKey(args, "relX") && HasKey(args, "relY") && main_window_) {
    POINT origin{0, 0};
    ClientToScreen(main_window_, &origin);
    const int rel_x = MulDiv(ReadInt(args, "relX", 0), static_cast<int>(dpi), 96);
    const int rel_y = MulDiv(ReadInt(args, "relY", 0), static_cast<int>(dpi), 96);
    const int px =
        std::clamp(static_cast<int>(origin.x) + rel_x, vx + 8, max(vx + 8, max_vx));
    const int py =
        std::clamp(static_cast<int>(origin.y) + rel_y, vy + 8, max(vy + 8, max_vy));
    return POINT{px, py};
  }

  RECT work{vx, vy, vx + vw, vy + vh};
  if (main_window_) {
    HMONITOR mon = MonitorFromWindow(main_window_, MONITOR_DEFAULTTONEAREST);
    MONITORINFO mi{};
    mi.cbSize = sizeof(mi);
    if (GetMonitorInfoW(mon, &mi)) {
      work = mi.rcWork;
    }
  }

  const int cascade = static_cast<int>(max<size_t>(1, windows_.size()) - 1) * 32;
  int pos_x = static_cast<int>(work.right) - px_w - 28 - cascade;
  int pos_y = static_cast<int>(work.top) + 64 + cascade;

  RECT main_rect{};
  if (main_window_ && GetWindowRect(main_window_, &main_rect)) {
    const int gap = 16;
    if (main_rect.right + gap + px_w <= work.right - 8) {
      pos_x = static_cast<int>(main_rect.right) + gap;
      pos_y = static_cast<int>(main_rect.top) + 48 + cascade;
    } else if (main_rect.left - gap - px_w >= work.left + 8) {
      pos_x = static_cast<int>(main_rect.left) - gap - px_w;
      pos_y = static_cast<int>(main_rect.top) + 48 + cascade;
    }
  }

  pos_x = std::clamp(pos_x, vx + 8, max(vx + 8, max_vx));
  pos_y = std::clamp(pos_y, vy + 8, max(vy + 8, max_vy));
  return POINT{pos_x, pos_y};
}

void DetachedWidgets::OpenOrUpdate(const flutter::EncodableMap& args) {
  const std::string id = ReadString(args, "id");
  if (id.empty()) return;

  auto& slot = windows_[id];
  const bool creating = (slot == nullptr || !IsWindow(slot->hwnd));
  if (creating) {
    slot = std::make_unique<DetachedWindow>();
    slot->id = id;
    slot->owner = this;
  }

  DetachedWindow* win = slot.get();
  win->kind = ReadString(args, "kind", "timer");
  win->title = Utf8ToWide(ReadString(args, "title", "Morrow"));
  win->primary_text = Utf8ToWide(ReadString(args, "primaryText"));
  win->secondary_text = Utf8ToWide(ReadString(args, "secondaryText"));
  win->badge_text = Utf8ToWide(ReadString(args, "badgeText"));
  win->progress = std::clamp(ReadDouble(args, "progress", 0.0), 0.0, 1.0);
  win->running = ReadBool(args, "running", false);
  const bool next_pinned = ReadBool(args, "pinned", true);
  const bool pin_changed = (!creating && next_pinned != win->pinned);
  win->pinned = next_pinned;
  ReadThemeTokens(args, win);

  if (creating) {
    const int req_w = std::clamp(ReadInt(args, "width", 320), 240, 640);
    const int req_h = std::clamp(ReadInt(args, "height", 205), 150, 480);
    const UINT dpi = main_window_ ? GetDpiForWindow(main_window_) : 96;
    const int px_w = MulDiv(req_w, static_cast<int>(dpi), 96);
    const int px_h = MulDiv(req_h, static_cast<int>(dpi), 96);
    const POINT pos = ComputeInitialPosition(args, px_w, px_h);

    const DWORD ex_style =
        WS_EX_TOOLWINDOW | WS_EX_LAYERED | (win->pinned ? WS_EX_TOPMOST : 0);
    win->hwnd = CreateWindowExW(
        ex_style, kDetachedClassName, win->title.c_str(),
        WS_POPUP | WS_THICKFRAME | WS_CLIPCHILDREN, pos.x, pos.y, px_w, px_h,
        nullptr, nullptr, GetModuleHandle(nullptr), win);
    if (win->hwnd) {
      ApplyWindowChrome(win);
      ShowWindow(win->hwnd, SW_SHOWNOACTIVATE);
      UpdateWindow(win->hwnd);
    }
  } else {
    ApplyWindowChrome(win);
    if (pin_changed) UpdateTopmostFlag(win);
    InvalidateRect(win->hwnd, nullptr, FALSE);
  }
}

void DetachedWidgets::OpenEditor(const flutter::EncodableMap& args) {
  const std::string id = ReadString(args, "id");
  if (id.empty()) return;

  auto& slot = windows_[id];
  const bool creating = (slot == nullptr || !IsWindow(slot->hwnd));
  if (creating) {
    slot = std::make_unique<DetachedWindow>();
    slot->id = id;
    slot->owner = this;
  }

  DetachedWindow* win = slot.get();
  win->kind = "editor";
  win->title = Utf8ToWide(ReadString(args, "windowTitle", "Morrow Studio"));
  const bool theme_only = ReadBool(args, "updateThemeOnly", false);
  const bool next_pinned = ReadBool(args, "pinned", true);
  const bool pin_changed = (!creating && next_pinned != win->pinned);
  win->pinned = next_pinned;
  ReadThemeTokens(args, win);

  if (creating || !theme_only) {
    win->category = ReadString(args, "category", "灵感");
    win->preview_mode = ReadBool(args, "previewMode", false);
    win->card_title = Utf8ToWide(ReadString(args, "title"));
    win->card_body = NormalizeToLf(Utf8ToWide(ReadString(args, "description")));
    win->card_todos = NormalizeToLf(Utf8ToWide(ReadString(args, "todos")));
    win->preview_scroll = 0.0f;
  }

  if (creating) {
    const int req_w = std::clamp(ReadInt(args, "width", 580), 420, 1200);
    const int req_h = std::clamp(ReadInt(args, "height", 500), 360, 960);
    const UINT dpi = main_window_ ? GetDpiForWindow(main_window_) : 96;
    const int px_w = MulDiv(req_w, static_cast<int>(dpi), 96);
    const int px_h = MulDiv(req_h, static_cast<int>(dpi), 96);
    const POINT pos = ComputeInitialPosition(args, px_w, px_h);

    const DWORD ex_style =
        WS_EX_APPWINDOW | WS_EX_LAYERED | (win->pinned ? WS_EX_TOPMOST : 0);
    win->hwnd = CreateWindowExW(
        ex_style, kDetachedClassName, win->title.c_str(),
        WS_POPUP | WS_THICKFRAME | WS_CLIPCHILDREN, pos.x, pos.y, px_w, px_h,
        nullptr, nullptr, GetModuleHandle(nullptr), win);
    if (win->hwnd) {
      CreateEditorControls(win, win->card_title, NormalizeToCrLf(win->card_body),
                           NormalizeToCrLf(win->card_todos));
      RefreshEditorFontsAndTheme(win);
      LayoutEditorControls(win);
      SyncEditControlVisibility(win);
      ApplyWindowChrome(win);
      SetWindowPos(win->hwnd, win->pinned ? HWND_TOPMOST : HWND_TOP, 0, 0, 0, 0,
                   SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
      BringWindowToTop(win->hwnd);
      SetForegroundWindow(win->hwnd);
      if (!win->preview_mode && win->title_edit) {
        SetFocus(win->title_edit);
        win->focused_edit = win->title_edit;
      } else {
        win->focused_edit = nullptr;
      }
      UpdateWindow(win->hwnd);
    }
  } else {
    RefreshEditorFontsAndTheme(win);
    ApplyWindowChrome(win);
    if (!theme_only) {
      SyncEditControlVisibility(win);
    }
    if (pin_changed) UpdateTopmostFlag(win);
    SetWindowTextW(win->hwnd, win->title.c_str());
    RedrawWindow(win->hwnd, nullptr, nullptr,
                 RDW_INVALIDATE | RDW_UPDATENOW | RDW_ALLCHILDREN);
  }
}

void DetachedWidgets::CreateEditorControls(DetachedWindow* win,
                                           const std::wstring& init_title,
                                           const std::wstring& init_body,
                                           const std::wstring& init_todos) {
  if (!win || !win->hwnd) return;
  HINSTANCE hinst = GetModuleHandle(nullptr);
  // No WS_VSCROLL on edit boxes: mouse-wheel scrolling works natively without
  // showing a classic grey Windows scrollbar track along the right edge.
  win->title_edit = CreateWindowExW(
      0, L"EDIT", init_title.c_str(),
      WS_CHILD | WS_TABSTOP | ES_AUTOHSCROLL, 0, 0, 100, 28, win->hwnd,
      reinterpret_cast<HMENU>(static_cast<INT_PTR>(kTitleEditId)), hinst,
      nullptr);
  win->body_edit = CreateWindowExW(
      0, L"EDIT", init_body.c_str(),
      WS_CHILD | WS_TABSTOP | ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN,
      0, 0, 100, 120, win->hwnd,
      reinterpret_cast<HMENU>(static_cast<INT_PTR>(kBodyEditId)), hinst,
      nullptr);
  win->todos_edit = CreateWindowExW(
      0, L"EDIT", init_todos.c_str(),
      WS_CHILD | WS_TABSTOP | ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN,
      0, 0, 100, 72, win->hwnd,
      reinterpret_cast<HMENU>(static_cast<INT_PTR>(kTodosEditId)), hinst,
      nullptr);
  SubclassEditForWheel(win->title_edit);
  SubclassEditForWheel(win->body_edit);
  SubclassEditForWheel(win->todos_edit);
}

void DetachedWidgets::SyncEditControlVisibility(DetachedWindow* win) {
  if (!win) return;
  const int show_cmd = win->preview_mode ? SW_HIDE : SW_SHOW;
  if (win->title_edit) ShowWindow(win->title_edit, show_cmd);
  if (win->body_edit) ShowWindow(win->body_edit, show_cmd);
  if (win->todos_edit) ShowWindow(win->todos_edit, show_cmd);
}

void DetachedWidgets::ToggleEditorPreview(DetachedWindow* win) {
  if (!win || !win->hwnd) return;
  if (win->preview_mode) {
    if (win->title_edit) SetWindowTextW(win->title_edit, win->card_title.c_str());
    if (win->body_edit) {
      SetWindowTextW(win->body_edit, NormalizeToCrLf(win->card_body).c_str());
    }
    if (win->todos_edit) {
      SetWindowTextW(win->todos_edit, NormalizeToCrLf(win->card_todos).c_str());
    }
    win->preview_mode = false;
    LayoutEditorControls(win);
    SyncEditControlVisibility(win);
    if (win->title_edit) {
      SetFocus(win->title_edit);
      win->focused_edit = win->title_edit;
    }
  } else {
    win->card_title = ReadEditText(win->title_edit);
    win->card_body = ReadEditText(win->body_edit);
    win->card_todos = ReadEditText(win->todos_edit);
    win->preview_mode = true;
    win->preview_scroll = 0.0f;
    win->focused_edit = nullptr;
    SyncEditControlVisibility(win);
  }
  RedrawWindow(win->hwnd, nullptr, nullptr,
               RDW_INVALIDATE | RDW_UPDATENOW | RDW_ALLCHILDREN);
}

void DetachedWidgets::RefreshEditorFontsAndTheme(DetachedWindow* win) {
  if (!win || !win->hwnd) return;
  const int dpi = static_cast<int>(GetDpiForWindow(win->hwnd));
  if (win->ui_font) {
    DeleteObject(win->ui_font);
    win->ui_font = nullptr;
  }
  if (win->title_font) {
    DeleteObject(win->title_font);
    win->title_font = nullptr;
  }
  if (win->edit_bg_brush) {
    DeleteObject(win->edit_bg_brush);
    win->edit_bg_brush = nullptr;
  }

  const wchar_t* face =
      win->font_family.empty() ? L"Microsoft YaHei UI" : win->font_family.c_str();
  win->ui_font = CreateFontW(
      -MulDiv(13, dpi, 96), 0, 0, 0, FW_NORMAL, FALSE, FALSE, FALSE,
      DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
      CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE, face);
  win->title_font = CreateFontW(
      -MulDiv(17, dpi, 96), 0, 0, 0, FW_SEMIBOLD, FALSE, FALSE, FALSE,
      DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
      CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE, face);

  const D2D1_COLOR_F surface = ArgbToD2D(win->surface_argb, 1.0f);
  const D2D1_COLOR_F bg = ArgbToD2D(win->bg_argb, 1.0f);
  const D2D1_COLOR_F ink = ArgbToD2D(win->ink_argb, 1.0f);
  const D2D1_COLOR_F edit_fill =
      LerpD2D(surface, bg, win->dark ? 0.35f : 0.12f, 1.0f);

  win->edit_bg_color = D2DToColorRef(edit_fill);
  win->edit_fg_color = D2DToColorRef(ink);
  win->edit_bg_brush = CreateSolidBrush(win->edit_bg_color);

  if (win->title_edit && win->title_font) {
    SendMessageW(win->title_edit, WM_SETFONT,
                 reinterpret_cast<WPARAM>(win->title_font), TRUE);
  }
  if (win->body_edit && win->ui_font) {
    SendMessageW(win->body_edit, WM_SETFONT,
                 reinterpret_cast<WPARAM>(win->ui_font), TRUE);
  }
  if (win->todos_edit && win->ui_font) {
    SendMessageW(win->todos_edit, WM_SETFONT,
                 reinterpret_cast<WPARAM>(win->ui_font), TRUE);
  }
}

DetachedWidgets::EditorLayout DetachedWidgets::ComputeEditorLayout(
    int w, int h, int dpi) const {
  auto dp = [dpi](int px) { return MulDiv(px, dpi, 96); };
  EditorLayout layout;
  layout.pad = dp(20);
  const int inner_w = max(120, w - layout.pad * 2);
  const int title_y = dp(98);
  const int title_h = dp(38);
  const int body_label_y = title_y + title_h + dp(10);
  const int body_y = body_label_y + dp(20);
  const int btm_h = dp(30);
  const int btm_y = h - layout.pad - btm_h;
  const int todos_gap = dp(14);
  const int todos_label_h = dp(20);
  const int todos_h = std::clamp(h / 5, dp(52), dp(120));
  const int todos_y = max(body_y + dp(84) + dp(10) + todos_label_h,
                          btm_y - todos_gap - todos_h);
  const int todos_label_y = todos_y - todos_label_h;
  const int body_h = max(dp(72), todos_label_y - dp(10) - body_y);

  const int inset_x = dp(12);
  const int inset_y = dp(8);
  layout.title_box =
      RECT{layout.pad, title_y, layout.pad + inner_w, title_y + title_h};
  layout.title_edit =
      RECT{layout.pad + inset_x, title_y + dp(6),
           layout.pad + inner_w - inset_x, title_y + title_h - dp(6)};
  layout.body_label =
      RECT{layout.pad, body_label_y, layout.pad + inner_w, body_y - dp(2)};
  layout.body_box =
      RECT{layout.pad, body_y, layout.pad + inner_w, body_y + body_h};
  layout.body_edit =
      RECT{layout.pad + inset_x, body_y + inset_y,
           layout.pad + inner_w - inset_x, body_y + body_h - inset_y};
  layout.todos_label =
      RECT{layout.pad, todos_label_y, layout.pad + inner_w, todos_y - dp(2)};
  layout.todos_box =
      RECT{layout.pad, todos_y, layout.pad + inner_w, todos_y + todos_h};
  layout.todos_edit =
      RECT{layout.pad + inset_x, todos_y + inset_y,
           layout.pad + inner_w - inset_x, todos_y + todos_h - inset_y};
  layout.btm_y = btm_y;
  layout.btm_h = btm_h;
  return layout;
}

void DetachedWidgets::LayoutEditorControls(DetachedWindow* win) {
  if (!win || !win->hwnd || win->kind != "editor") return;
  RECT rc{};
  GetClientRect(win->hwnd, &rc);
  const int w = static_cast<int>(max(240L, rc.right - rc.left));
  const int h = static_cast<int>(max(240L, rc.bottom - rc.top));
  const int dpi = static_cast<int>(GetDpiForWindow(win->hwnd));
  const EditorLayout layout = ComputeEditorLayout(w, h, dpi);

  auto place = [](HWND ctrl, const RECT& r) {
    if (!ctrl) return;
    MoveWindow(ctrl, r.left, r.top, max(40L, r.right - r.left),
               max(18L, r.bottom - r.top), TRUE);
  };
  place(win->title_edit, layout.title_edit);
  place(win->body_edit, layout.body_edit);
  place(win->todos_edit, layout.todos_edit);
}

void DetachedWidgets::DestroyWindowResources(DetachedWindow* win) {
  if (!win) return;
  if (win->ui_font) {
    DeleteObject(win->ui_font);
    win->ui_font = nullptr;
  }
  if (win->title_font) {
    DeleteObject(win->title_font);
    win->title_font = nullptr;
  }
  if (win->edit_bg_brush) {
    DeleteObject(win->edit_bg_brush);
    win->edit_bg_brush = nullptr;
  }
}

void DetachedWidgets::CloseById(const std::string& id) {
  const auto it = windows_.find(id);
  if (it == windows_.end()) return;
  auto owned = std::move(it->second);
  windows_.erase(it);
  HWND hwnd = owned ? owned->hwnd : nullptr;
  if (owned) {
    owned->owner = nullptr;
    owned->hwnd = nullptr;
  }
  if (hwnd && IsWindow(hwnd)) {
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    DestroyWindow(hwnd);
  }
  DestroyWindowResources(owned.get());
}

void DetachedWidgets::CloseAll() {
  auto old = std::move(windows_);
  windows_.clear();
  for (auto& pair : old) {
    if (!pair.second) continue;
    HWND hwnd = pair.second->hwnd;
    pair.second->owner = nullptr;
    pair.second->hwnd = nullptr;
    if (hwnd && IsWindow(hwnd)) {
      SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
      DestroyWindow(hwnd);
    }
    DestroyWindowResources(pair.second.get());
  }
}

void DetachedWidgets::EmitAction(const std::string& id,
                                 const std::string& action) {
  if (!channel_) return;
  flutter::EncodableMap payload{
      {flutter::EncodableValue("id"), flutter::EncodableValue(id)},
      {flutter::EncodableValue("action"), flutter::EncodableValue(action)},
  };
  channel_->InvokeMethod("onWidgetAction",
                         std::make_unique<flutter::EncodableValue>(payload));
}

void DetachedWidgets::EmitEditorAction(DetachedWindow* win,
                                       const std::string& action) {
  if (!channel_ || !win) return;
  if (!win->preview_mode) {
    win->card_title = ReadEditText(win->title_edit);
    win->card_body = ReadEditText(win->body_edit);
    win->card_todos = ReadEditText(win->todos_edit);
  }
  flutter::EncodableMap payload{
      {flutter::EncodableValue("id"), flutter::EncodableValue(win->id)},
      {flutter::EncodableValue("action"), flutter::EncodableValue(action)},
      {flutter::EncodableValue("title"),
       flutter::EncodableValue(WideToUtf8(win->card_title))},
      {flutter::EncodableValue("description"),
       flutter::EncodableValue(WideToUtf8(win->card_body))},
      {flutter::EncodableValue("category"),
       flutter::EncodableValue(win->category)},
      {flutter::EncodableValue("todos"),
       flutter::EncodableValue(WideToUtf8(win->card_todos))},
  };
  channel_->InvokeMethod("onEditorAction",
                         std::make_unique<flutter::EncodableValue>(payload));
}

void DetachedWidgets::PaintWindow(DetachedWindow* win) {
  if (!win || !win->hwnd) return;
  PAINTSTRUCT ps{};
  HDC hdc = BeginPaint(win->hwnd, &ps);
  RECT rc{};
  GetClientRect(win->hwnd, &rc);
  const int w = static_cast<int>(max(1L, rc.right - rc.left));
  const int h = static_cast<int>(max(1L, rc.bottom - rc.top));

  HDC mem_dc = CreateCompatibleDC(hdc);
  HBITMAP bmp = CreateCompatibleBitmap(hdc, w, h);
  HGDIOBJ old_bmp = SelectObject(mem_dc, bmp);

  if (EnsureD2DFactories() && SUCCEEDED(dc_target_->BindDC(mem_dc, &rc))) {
    const float scale = static_cast<float>(GetDpiForWindow(win->hwnd)) / 96.0f;
    dc_target_->BeginDraw();
    dc_target_->SetTransform(D2D1::Matrix3x2F::Identity());
    if (win->kind == "editor") {
      if (win->preview_mode) {
        PaintCardPreview(dc_target_, win, w, h, scale);
      } else {
        PaintEditorContent(dc_target_, win, w, h, scale);
      }
    } else {
      PaintWidgetContent(dc_target_, win, w, h, scale);
    }
    const HRESULT hr = dc_target_->EndDraw();
    if (hr == D2DERR_RECREATE_TARGET) {
      SafeRelease(dc_target_);
    }
  }

  BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);
  SelectObject(mem_dc, old_bmp);
  DeleteObject(bmp);
  DeleteDC(mem_dc);
  EndPaint(win->hwnd, &ps);
}

void DetachedWidgets::PaintGlassBackdrop(ID2D1RenderTarget* rt,
                                           DetachedWindow* win, int w, int h,
                                           float scale) {
  const D2D1_COLOR_F surface = ArgbToD2D(win->surface_argb, 1.0f);
  const D2D1_COLOR_F bg = ArgbToD2D(win->bg_argb, 1.0f);
  const D2D1_COLOR_F accent = ArgbToD2D(win->accent_argb, 1.0f);
  const D2D1_COLOR_F edge = ArgbToD2D(win->edge_argb);

  const float fw = static_cast<float>(w);
  const float fh = static_cast<float>(h);
  const D2D1_RECT_F full = D2D1::RectF(0.0f, 0.0f, fw, fh);

  rt->Clear(LerpD2D(surface, bg, win->dark ? 0.20f : 0.10f, 1.0f));

  // Diagonal Glass linear gradient matching appearance.dart Glass surface
  D2D1_GRADIENT_STOP stops[2] = {
      {0.0f, surface},
      {1.0f, LerpD2D(surface, bg, win->dark ? 0.42f : 0.34f, 1.0f)},
  };
  ID2D1GradientStopCollection* stop_coll = nullptr;
  if (SUCCEEDED(rt->CreateGradientStopCollection(stops, 2, &stop_coll)) &&
      stop_coll) {
    ID2D1LinearGradientBrush* grad = nullptr;
    if (SUCCEEDED(rt->CreateLinearGradientBrush(
            D2D1::LinearGradientBrushProperties(D2D1::Point2F(0.0f, 0.0f),
                                                D2D1::Point2F(fw, fh)),
            stop_coll, &grad)) &&
        grad) {
      rt->FillRectangle(full, grad);
      grad->Release();
    }
    stop_coll->Release();
  }

  // Soft top-left ambient accent glow
  D2D1_GRADIENT_STOP glow_stops[2] = {
      {0.0f, D2D1::ColorF(accent.r, accent.g, accent.b,
                          win->dark ? 0.11f : 0.07f)},
      {1.0f, D2D1::ColorF(accent.r, accent.g, accent.b, 0.0f)},
  };
  ID2D1GradientStopCollection* glow_coll = nullptr;
  if (SUCCEEDED(rt->CreateGradientStopCollection(glow_stops, 2, &glow_coll)) &&
      glow_coll) {
    ID2D1RadialGradientBrush* radial = nullptr;
    if (SUCCEEDED(rt->CreateRadialGradientBrush(
            D2D1::RadialGradientBrushProperties(
                D2D1::Point2F(fw * 0.15f, fh * 0.12f), D2D1::Point2F(0.0f, 0.0f),
                max(160.0f, fw * 0.72f), max(140.0f, fh * 0.68f)),
            glow_coll, &radial)) &&
        radial) {
      rt->FillRectangle(full, radial);
      radial->Release();
    }
    glow_coll->Release();
  }

  // Outer Glass edge + top specular highlight
  const float r = ScaleRadius(14.0f, win->corner_radius, scale);
  const float edge_alpha = max(win->dark ? 0.22f : 0.14f, edge.a);
  ID2D1SolidColorBrush* edge_brush = nullptr;
  if (SUCCEEDED(rt->CreateSolidColorBrush(
          D2D1::ColorF(edge.r, edge.g, edge.b, edge_alpha), &edge_brush)) &&
      edge_brush) {
    rt->DrawRoundedRectangle(
        D2D1::RoundedRect(D2D1::RectF(0.5f, 0.5f, fw - 0.5f, fh - 0.5f), r,
                          r),
        edge_brush, max(1.0f, 1.0f * scale));
    edge_brush->Release();
  }

  ID2D1SolidColorBrush* spec_brush = nullptr;
  if (SUCCEEDED(rt->CreateSolidColorBrush(
          D2D1::ColorF(1.0f, 1.0f, 1.0f, win->dark ? 0.10f : 0.40f),
          &spec_brush)) &&
      spec_brush) {
    rt->DrawLine(D2D1::Point2F(max(10.0f, r), 1.5f * scale),
                 D2D1::Point2F(fw - max(10.0f, r), 1.5f * scale), spec_brush,
                 max(1.0f, 1.0f * scale));
    spec_brush->Release();
  }
}

void DetachedWidgets::PaintWidgetContent(ID2D1RenderTarget* rt,
                                         DetachedWindow* win, int w, int h,
                                         float scale) {
  PaintGlassBackdrop(rt, win, w, h, scale);

  const D2D1_COLOR_F surface = ArgbToD2D(win->surface_argb, 1.0f);
  const D2D1_COLOR_F bg = ArgbToD2D(win->bg_argb, 1.0f);
  const D2D1_COLOR_F ink = ArgbToD2D(win->ink_argb, 1.0f);
  const D2D1_COLOR_F muted = ArgbToD2D(win->muted_argb, 1.0f);
  const D2D1_COLOR_F accent = ArgbToD2D(win->accent_argb, 1.0f);
  const D2D1_COLOR_F on_accent = ArgbToD2D(win->on_accent_argb, 1.0f);
  const D2D1_COLOR_F raw_line = ArgbToD2D(win->line_argb);
  const D2D1_COLOR_F line{raw_line.r, raw_line.g, raw_line.b,
                          max(win->dark ? 0.14f : 0.08f, raw_line.a)};

  const float fw = static_cast<float>(w);
  const float fh = static_cast<float>(h);
  const float pad = 16.0f * scale;
  const float inner_w = max(100.0f, fw - pad * 2.0f);
  const float r_card = ScaleRadius(14.0f, win->corner_radius, scale);
  const float r_pill = ScaleRadius(9.0f, win->corner_radius, scale);

  ID2D1SolidColorBrush* ink_b = nullptr;
  ID2D1SolidColorBrush* muted_b = nullptr;
  ID2D1SolidColorBrush* accent_b = nullptr;
  ID2D1SolidColorBrush* on_accent_b = nullptr;
  ID2D1SolidColorBrush* line_b = nullptr;
  rt->CreateSolidColorBrush(ink, &ink_b);
  rt->CreateSolidColorBrush(muted, &muted_b);
  rt->CreateSolidColorBrush(accent, &accent_b);
  rt->CreateSolidColorBrush(on_accent, &on_accent_b);
  rt->CreateSolidColorBrush(line, &line_b);

  IDWriteTextFormat* eyebrow_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 9.0f * scale, DWRITE_FONT_WEIGHT_NORMAL,
      true);
  IDWriteTextFormat* title_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 12.5f * scale,
      DWRITE_FONT_WEIGHT_SEMI_BOLD, true);
  IDWriteTextFormat* primary_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family,
      (win->kind == "timer" ? 28.0f : 19.0f) * scale, DWRITE_FONT_WEIGHT_BOLD,
      true);
  IDWriteTextFormat* sec_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 11.5f * scale,
      DWRITE_FONT_WEIGHT_NORMAL, true);
  IDWriteTextFormat* btn_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 11.0f * scale,
      DWRITE_FONT_WEIGHT_SEMI_BOLD, true, DWRITE_TEXT_ALIGNMENT_CENTER,
      DWRITE_PARAGRAPH_ALIGNMENT_CENTER);

  // Header icon badge + eyebrow + title
  const float badge_sz = 26.0f * scale;
  const D2D1_RECT_F badge_rect =
      D2D1::RectF(pad, pad - 2.0f * scale, pad + badge_sz, pad - 2.0f * scale + badge_sz);
  ID2D1SolidColorBrush* badge_bg = nullptr;
  if (SUCCEEDED(rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, 0.14f), &badge_bg)) &&
      badge_bg) {
    const float br = ScaleRadius(9.0f, win->corner_radius, scale);
    rt->FillRoundedRectangle(D2D1::RoundedRect(badge_rect, br, br), badge_bg);
    badge_bg->Release();
  }
  DrawD2DKindIcon(rt, win->kind, badge_rect, accent);

  const float title_x = pad + badge_sz + 9.0f * scale;
  const float title_right = fw - pad - 64.0f * scale;
  if (eyebrow_fmt && muted_b) {
    rt->DrawText(L"MORROW / STUDIO", 15, eyebrow_fmt,
                 D2D1::RectF(title_x, pad - 3.5f * scale, max(title_x + 40.0f, title_right),
                             pad + 10.5f * scale),
                 muted_b);
  }
  if (title_fmt && ink_b) {
    rt->DrawText(win->title.c_str(), static_cast<UINT32>(win->title.size()),
                 title_fmt,
                 D2D1::RectF(title_x, pad + 8.0f * scale, max(title_x + 40.0f, title_right),
                             pad + 27.0f * scale),
                 ink_b);
  }

  win->buttons.clear();
  const float hdr_btn_sz = 26.0f * scale;
  const float btn_h = 26.0f * scale;
  const D2D1_RECT_F close_rect = D2D1::RectF(
      fw - pad - hdr_btn_sz, pad - 2.0f * scale, fw - pad,
      pad - 2.0f * scale + hdr_btn_sz);
  const D2D1_RECT_F pin_rect = D2D1::RectF(
      close_rect.left - 6.0f * scale - hdr_btn_sz, close_rect.top,
      close_rect.left - 6.0f * scale, close_rect.bottom);

  auto draw_icon_btn = [&](D2D1_RECT_F rect, const std::string& icon_kind,
                           const std::string& action, bool tonal_active,
                           bool filled_icon) {
    const bool hovered = (win->hovered_action == action);
    if (tonal_active) {
      ID2D1SolidColorBrush* fb = nullptr;
      ID2D1SolidColorBrush* ab = nullptr;
      rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, hovered ? 0.22f : 0.14f), &fb);
      rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, 0.42f), &ab);
      if (fb) {
        rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
        fb->Release();
      }
      if (ab) {
        rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), ab,
                                 max(1.0f, 1.0f * scale));
        ab->Release();
      }
    } else {
      const D2D1_COLOR_F f = LerpD2D(surface, bg, hovered ? 0.12f : 0.24f,
                                     hovered ? 0.90f : (win->dark ? 0.48f : 0.68f));
      ID2D1SolidColorBrush* fb = nullptr;
      if (SUCCEEDED(rt->CreateSolidColorBrush(f, &fb)) && fb) {
        rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
        fb->Release();
      }
      if (line_b) {
        rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill),
                                 line_b, max(1.0f, 1.0f * scale));
      }
    }
    DrawD2DActionIcon(rt, icon_kind, rect, tonal_active ? accent : muted,
                      filled_icon);
    win->buttons.push_back(ActionRect{
        RECT{static_cast<LONG>(rect.left), static_cast<LONG>(rect.top),
             static_cast<LONG>(rect.right), static_cast<LONG>(rect.bottom)},
        action,
        ""});
  };

  auto draw_pill = [&](D2D1_RECT_F rect, const wchar_t* label,
                       const std::string& action, bool filled_primary,
                       bool tonal_active) {
    const bool hovered = (win->hovered_action == action);
    if (filled_primary && accent_b) {
      rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), accent_b);
    } else if (tonal_active) {
      ID2D1SolidColorBrush* fb = nullptr;
      ID2D1SolidColorBrush* ab = nullptr;
      rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, hovered ? 0.22f : 0.15f), &fb);
      rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, 0.45f), &ab);
      if (fb) {
        rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
        fb->Release();
      }
      if (ab) {
        rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), ab,
                                 max(1.0f, 1.0f * scale));
        ab->Release();
      }
    } else {
      const D2D1_COLOR_F f = LerpD2D(surface, bg, hovered ? 0.12f : 0.26f,
                                     hovered ? 0.92f : (win->dark ? 0.55f : 0.72f));
      ID2D1SolidColorBrush* fb = nullptr;
      if (SUCCEEDED(rt->CreateSolidColorBrush(f, &fb)) && fb) {
        rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
        fb->Release();
      }
      if (line_b) {
        rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill),
                                 line_b, max(1.0f, 1.0f * scale));
      }
    }
    ID2D1SolidColorBrush* tb =
        filled_primary ? on_accent_b : (tonal_active ? accent_b : ink_b);
    if (btn_fmt && tb) {
      rt->DrawText(label, static_cast<UINT32>(wcslen(label)), btn_fmt, rect, tb);
    }
    win->buttons.push_back(ActionRect{
        RECT{static_cast<LONG>(rect.left), static_cast<LONG>(rect.top),
             static_cast<LONG>(rect.right), static_cast<LONG>(rect.bottom)},
        action,
        ""});
  };

  draw_icon_btn(pin_rect, "pin", "togglePin", win->pinned, win->pinned);
  draw_icon_btn(close_rect, "dock", "dock", false, false);

  const float divider_y = pad + badge_sz + 7.0f * scale;
  if (line_b) {
    rt->DrawLine(D2D1::Point2F(pad, divider_y),
                 D2D1::Point2F(fw - pad, divider_y), line_b,
                 max(1.0f, 1.0f * scale));
  }

  // Inner surface container matching WorkbenchTimerCard
  const float footer_reserve = win->kind == "timer" ? (btn_h + 10.0f * scale) : 0.0f;
  const float card_top = divider_y + 10.0f * scale;
  const float card_h = max(68.0f * scale, fh - card_top - pad - footer_reserve);
  const D2D1_RECT_F card_rect =
      D2D1::RectF(pad, card_top, pad + inner_w, card_top + card_h);
  ID2D1SolidColorBrush* card_bg = nullptr;
  if (SUCCEEDED(rt->CreateSolidColorBrush(
          LerpD2D(surface, bg, win->dark ? 0.34f : 0.18f,
                  win->dark ? 0.62f : 0.72f),
          &card_bg)) &&
      card_bg) {
    rt->FillRoundedRectangle(D2D1::RoundedRect(card_rect, r_card, r_card),
                             card_bg);
    card_bg->Release();
  }
  if (line_b) {
    rt->DrawRoundedRectangle(D2D1::RoundedRect(card_rect, r_card, r_card),
                             line_b, max(1.0f, 1.0f * scale));
  }

  const float c_pad = 12.0f * scale;
  if (primary_fmt && ink_b) {
    rt->DrawText(win->primary_text.c_str(),
                 static_cast<UINT32>(win->primary_text.size()), primary_fmt,
                 D2D1::RectF(card_rect.left + c_pad, card_rect.top + 10.0f * scale,
                             card_rect.right - c_pad,
                             card_rect.top + 48.0f * scale),
                 ink_b);
  }
  const std::wstring line2 =
      win->badge_text.empty()
          ? win->secondary_text
          : (win->secondary_text.empty()
                 ? win->badge_text
                 : win->badge_text + L"  ·  " + win->secondary_text);
  if (sec_fmt && muted_b) {
    rt->DrawText(line2.c_str(), static_cast<UINT32>(line2.size()), sec_fmt,
                 D2D1::RectF(card_rect.left + c_pad, card_rect.top + 48.0f * scale,
                             card_rect.right - c_pad,
                             card_rect.top + 68.0f * scale),
                 muted_b);
  }

  if (win->kind == "timer") {
    const D2D1_RECT_F track = D2D1::RectF(
        card_rect.left + c_pad, card_rect.bottom - 12.0f * scale,
        card_rect.right - c_pad, card_rect.bottom - 7.5f * scale);
    if (line_b) {
      rt->FillRoundedRectangle(
          D2D1::RoundedRect(track, 2.2f * scale, 2.2f * scale), line_b);
    }
    const float fill_w =
        max(0.0f, (track.right - track.left) * static_cast<float>(win->progress));
    if (fill_w >= 2.0f && accent_b) {
      rt->FillRoundedRectangle(
          D2D1::RoundedRect(
              D2D1::RectF(track.left, track.top, track.left + fill_w,
                          track.bottom),
              2.2f * scale, 2.2f * scale),
          accent_b);
    }

    const float act_y = fh - pad - btn_h;
    const float toggle_w = 78.0f * scale;
    const float reset_w = 58.0f * scale;
    draw_pill(D2D1::RectF(pad, act_y, pad + toggle_w, act_y + btn_h),
              win->running ? L"暂停" : L"开始", "toggle", !win->running,
              win->running);
    draw_pill(D2D1::RectF(pad + toggle_w + 8.0f * scale, act_y,
                          pad + toggle_w + 8.0f * scale + reset_w, act_y + btn_h),
              L"重置", "reset", false, false);
  }

  DrawResizeGrip(rt, fw, fh, scale, muted);

  SafeRelease(btn_fmt);
  SafeRelease(sec_fmt);
  SafeRelease(primary_fmt);
  SafeRelease(title_fmt);
  SafeRelease(eyebrow_fmt);
  SafeRelease(line_b);
  SafeRelease(on_accent_b);
  SafeRelease(accent_b);
  SafeRelease(muted_b);
  SafeRelease(ink_b);
}

void DetachedWidgets::PaintCardPreview(ID2D1RenderTarget* rt,
                                       DetachedWindow* win, int w, int h,
                                       float scale) {
  PaintGlassBackdrop(rt, win, w, h, scale);

  const D2D1_COLOR_F surface = ArgbToD2D(win->surface_argb, 1.0f);
  const D2D1_COLOR_F bg = ArgbToD2D(win->bg_argb, 1.0f);
  const D2D1_COLOR_F ink = ArgbToD2D(win->ink_argb, 1.0f);
  const D2D1_COLOR_F muted = ArgbToD2D(win->muted_argb, 1.0f);
  const D2D1_COLOR_F accent = ArgbToD2D(win->accent_argb, 1.0f);
  const D2D1_COLOR_F on_accent = ArgbToD2D(win->on_accent_argb, 1.0f);
  const D2D1_COLOR_F raw_line = ArgbToD2D(win->line_argb);
  const D2D1_COLOR_F line{raw_line.r, raw_line.g, raw_line.b,
                          max(win->dark ? 0.14f : 0.08f, raw_line.a)};

  const float fw = static_cast<float>(w);
  const float fh = static_cast<float>(h);
  const float pad = 22.0f * scale;
  const float inner_w = max(120.0f, fw - pad * 2.0f);
  const float r_pill = ScaleRadius(10.0f, win->corner_radius, scale);
  const float r_card = ScaleRadius(16.0f, win->corner_radius, scale);

  ID2D1SolidColorBrush* ink_b = nullptr;
  ID2D1SolidColorBrush* muted_b = nullptr;
  ID2D1SolidColorBrush* accent_b = nullptr;
  ID2D1SolidColorBrush* on_accent_b = nullptr;
  ID2D1SolidColorBrush* line_b = nullptr;
  rt->CreateSolidColorBrush(ink, &ink_b);
  rt->CreateSolidColorBrush(muted, &muted_b);
  rt->CreateSolidColorBrush(accent, &accent_b);
  rt->CreateSolidColorBrush(on_accent, &on_accent_b);
  rt->CreateSolidColorBrush(line, &line_b);

  IDWriteTextFormat* eyebrow_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 9.0f * scale, DWRITE_FONT_WEIGHT_NORMAL,
      true);
  IDWriteTextFormat* hdr_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 15.5f * scale,
      DWRITE_FONT_WEIGHT_SEMI_BOLD, true);
  IDWriteTextFormat* card_title_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 20.0f * scale,
      DWRITE_FONT_WEIGHT_BOLD, false);
  IDWriteTextFormat* h2_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 15.0f * scale,
      DWRITE_FONT_WEIGHT_BOLD, false);
  IDWriteTextFormat* body_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 13.5f * scale,
      DWRITE_FONT_WEIGHT_NORMAL, false);
  IDWriteTextFormat* label_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 11.5f * scale,
      DWRITE_FONT_WEIGHT_SEMI_BOLD, true);
  IDWriteTextFormat* btn_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 11.5f * scale,
      DWRITE_FONT_WEIGHT_SEMI_BOLD, true, DWRITE_TEXT_ALIGNMENT_CENTER,
      DWRITE_PARAGRAPH_ALIGNMENT_CENTER);

  // Header icon + eyebrow + title
  const float badge_sz = 34.0f * scale;
  const D2D1_RECT_F badge_rect =
      D2D1::RectF(pad, 14.0f * scale, pad + badge_sz, 14.0f * scale + badge_sz);
  ID2D1SolidColorBrush* badge_bg = nullptr;
  if (SUCCEEDED(rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, 0.14f), &badge_bg)) &&
      badge_bg) {
    const float br = ScaleRadius(11.0f, win->corner_radius, scale);
    rt->FillRoundedRectangle(D2D1::RoundedRect(badge_rect, br, br), badge_bg);
    badge_bg->Release();
  }
  DrawD2DKindIcon(rt, "editor", badge_rect, accent);

  const float title_x = pad + badge_sz + 12.0f * scale;
  const float title_right = fw - pad - 172.0f * scale;
  if (eyebrow_fmt && muted_b) {
    rt->DrawText(L"MORROW / A LITTLE POSSIBILITY", 27, eyebrow_fmt,
                 D2D1::RectF(title_x, 12.5f * scale, max(title_x + 40.0f, title_right),
                             26.5f * scale),
                 muted_b);
  }
  const std::wstring preview_hdr =
      win->card_title.empty() ? win->title : win->card_title;
  if (hdr_fmt && ink_b) {
    rt->DrawText(preview_hdr.c_str(), static_cast<UINT32>(preview_hdr.size()),
                 hdr_fmt,
                 D2D1::RectF(title_x, 26.5f * scale, max(title_x + 40.0f, title_right),
                             51.0f * scale),
                 ink_b);
  }

  win->buttons.clear();
  auto draw_btn = [&](D2D1_RECT_F rect, const wchar_t* label,
                      const std::string& action, const std::string& value,
                      bool primary, bool highlighted) {
    const bool hovered =
        (win->hovered_action == action && win->hovered_value == value);
    if (primary && accent_b) {
      rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), accent_b);
    } else if (highlighted) {
      ID2D1SolidColorBrush* fb = nullptr;
      ID2D1SolidColorBrush* ab = nullptr;
      rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, hovered ? 0.22f : 0.15f), &fb);
      rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, 0.45f), &ab);
      if (fb) {
        rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
        fb->Release();
      }
      if (ab) {
        rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), ab,
                                 max(1.0f, 1.0f * scale));
        ab->Release();
      }
    } else {
      const D2D1_COLOR_F f = LerpD2D(surface, bg, hovered ? 0.12f : 0.24f,
                                     hovered ? 0.92f : (win->dark ? 0.55f : 0.76f));
      ID2D1SolidColorBrush* fb = nullptr;
      if (SUCCEEDED(rt->CreateSolidColorBrush(f, &fb)) && fb) {
        rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
        fb->Release();
      }
      if (line_b) {
        rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill),
                                 line_b, max(1.0f, 1.0f * scale));
      }
    }
    ID2D1SolidColorBrush* tb =
        primary ? on_accent_b : (highlighted ? accent_b : ink_b);
    if (btn_fmt && tb) {
      rt->DrawText(label, static_cast<UINT32>(wcslen(label)), btn_fmt, rect, tb);
    }
    win->buttons.push_back(ActionRect{
        RECT{static_cast<LONG>(rect.left), static_cast<LONG>(rect.top),
             static_cast<LONG>(rect.right), static_cast<LONG>(rect.bottom)},
        action,
        value});
  };
  auto draw_hdr_icon = [&](D2D1_RECT_F rect, const std::string& icon_kind,
                           const std::string& action, bool highlighted,
                           bool filled_icon) {
    const bool hovered = (win->hovered_action == action);
    if (highlighted) {
      ID2D1SolidColorBrush* fb = nullptr;
      ID2D1SolidColorBrush* ab = nullptr;
      rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, hovered ? 0.22f : 0.15f), &fb);
      rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, 0.45f), &ab);
      if (fb) {
        rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
        fb->Release();
      }
      if (ab) {
        rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), ab,
                                 max(1.0f, 1.0f * scale));
        ab->Release();
      }
    } else {
      const D2D1_COLOR_F f = LerpD2D(surface, bg, hovered ? 0.12f : 0.24f,
                                     hovered ? 0.92f : (win->dark ? 0.50f : 0.72f));
      ID2D1SolidColorBrush* fb = nullptr;
      if (SUCCEEDED(rt->CreateSolidColorBrush(f, &fb)) && fb) {
        rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
        fb->Release();
      }
      if (line_b) {
        rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill),
                                 line_b, max(1.0f, 1.0f * scale));
      }
    }
    DrawD2DActionIcon(rt, icon_kind, rect, highlighted ? accent : muted,
                      filled_icon);
    win->buttons.push_back(ActionRect{
        RECT{static_cast<LONG>(rect.left), static_cast<LONG>(rect.top),
             static_cast<LONG>(rect.right), static_cast<LONG>(rect.bottom)},
        action,
        ""});
  };
  auto draw_hdr_mode_pill = [&](D2D1_RECT_F rect, const std::string& icon_kind,
                                const wchar_t* label) {
    const bool hovered = (win->hovered_action == "togglePreview");
    ID2D1SolidColorBrush* fb = nullptr;
    ID2D1SolidColorBrush* ab = nullptr;
    rt->CreateSolidColorBrush(
        D2D1::ColorF(accent.r, accent.g, accent.b, hovered ? 0.22f : 0.14f), &fb);
    rt->CreateSolidColorBrush(
        D2D1::ColorF(accent.r, accent.g, accent.b, 0.42f), &ab);
    if (fb) {
      rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
      fb->Release();
    }
    if (ab) {
      rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), ab,
                               max(1.0f, 1.0f * scale));
      ab->Release();
    }
    DrawD2DActionIcon(
        rt, icon_kind,
        D2D1::RectF(rect.left + 5.0f * scale, rect.top + 3.0f * scale,
                    rect.left + 23.0f * scale, rect.bottom - 3.0f * scale),
        accent, false);
    if (btn_fmt && accent_b) {
      rt->DrawText(
          label, static_cast<UINT32>(wcslen(label)), btn_fmt,
          D2D1::RectF(rect.left + 20.0f * scale, rect.top, rect.right - 4.0f * scale,
                      rect.bottom),
          accent_b);
    }
    win->buttons.push_back(ActionRect{
        RECT{static_cast<LONG>(rect.left), static_cast<LONG>(rect.top),
             static_cast<LONG>(rect.right), static_cast<LONG>(rect.bottom)},
        "togglePreview",
        ""});
  };

  // Top bar: [Edit pill] [Pin icon] [Dock icon] [Close icon]
  const float top_sz = 28.0f * scale;
  const float top_y = 17.0f * scale;
  const float edit_w = 58.0f * scale;
  const float gap = 6.0f * scale;
  const float close_x = fw - pad - top_sz;
  const float dock_x = close_x - gap - top_sz;
  const float pin_x = dock_x - gap - top_sz;
  const float edit_x = pin_x - gap - edit_w;
  draw_hdr_icon(D2D1::RectF(close_x, top_y, close_x + top_sz, top_y + top_sz),
                "close", "close", false, false);
  draw_hdr_icon(D2D1::RectF(dock_x, top_y, dock_x + top_sz, top_y + top_sz),
                "dock", "dock", false, false);
  draw_hdr_icon(D2D1::RectF(pin_x, top_y, pin_x + top_sz, top_y + top_sz),
                "pin", "togglePin", win->pinned, win->pinned);
  draw_hdr_mode_pill(D2D1::RectF(edit_x, top_y, edit_x + edit_w, top_y + top_sz),
                     "edit", L"编辑");

  const float hdr_div_y = 58.0f * scale;
  if (line_b) {
    rt->DrawLine(D2D1::Point2F(pad, hdr_div_y),
                 D2D1::Point2F(fw - pad, hdr_div_y), line_b,
                 max(1.0f, 1.0f * scale));
  }

  // Preview card container (scrollable via mouse wheel with ZERO scrollbar!)
  const float btm_h = 30.0f * scale;
  const float btm_y = fh - pad - btm_h;
  const float card_top = hdr_div_y + 12.0f * scale;
  const float card_bot = btm_y - 16.0f * scale;
  const D2D1_RECT_F card_rect = D2D1::RectF(pad, card_top, pad + inner_w, card_bot);

  ID2D1SolidColorBrush* card_bg = nullptr;
  if (SUCCEEDED(rt->CreateSolidColorBrush(
          LerpD2D(surface, bg, win->dark ? 0.30f : 0.14f,
                  win->dark ? 0.65f : 0.78f),
          &card_bg)) &&
      card_bg) {
    rt->FillRoundedRectangle(D2D1::RoundedRect(card_rect, r_card, r_card),
                             card_bg);
    card_bg->Release();
  }
  if (line_b) {
    rt->DrawRoundedRectangle(D2D1::RoundedRect(card_rect, r_card, r_card),
                             line_b, max(1.0f, 1.0f * scale));
  }

  // Clipped scrollable preview content
  const D2D1_RECT_F clip_rect = D2D1::RectF(
      card_rect.left + 1.0f, card_rect.top + 8.0f * scale,
      card_rect.right - 1.0f, card_rect.bottom - 8.0f * scale);
  rt->PushAxisAlignedClip(clip_rect, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);

  const float cp = 18.0f * scale;
  const float text_x = card_rect.left + cp;
  const float text_w = max(80.0f, (card_rect.right - card_rect.left) - cp * 2.0f);
  float cur_y = card_rect.top + 16.0f * scale - win->preview_scroll;
  const float start_content_y = card_rect.top + 16.0f * scale;

  // Category tag
  const std::wstring cat_w = Utf8ToWide(win->category);
  const float cat_w_px = max(48.0f, static_cast<float>(cat_w.size()) * 14.0f) * scale;
  const D2D1_RECT_F cat_rect =
      D2D1::RectF(text_x, cur_y, text_x + cat_w_px, cur_y + 20.0f * scale);
  ID2D1SolidColorBrush* cat_bg = nullptr;
  if (SUCCEEDED(rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, 0.14f), &cat_bg)) &&
      cat_bg) {
    rt->FillRoundedRectangle(
        D2D1::RoundedRect(cat_rect, 6.0f * scale, 6.0f * scale), cat_bg);
    cat_bg->Release();
  }
  if (btn_fmt && accent_b) {
    rt->DrawText(cat_w.c_str(), static_cast<UINT32>(cat_w.size()), btn_fmt,
                 cat_rect, accent_b);
  }
  cur_y += 28.0f * scale;

  auto draw_wrapped = [&](const std::wstring& txt, IDWriteTextFormat* fmt,
                          ID2D1SolidColorBrush* br, float x_off, float gap_after) {
    if (txt.empty() || !fmt || !br) {
      cur_y += 10.0f * scale;
      return;
    }
    IDWriteTextLayout* layout = nullptr;
    const float avail_w = max(40.0f, text_w - x_off);
    if (SUCCEEDED(dwrite_factory_->CreateTextLayout(
            txt.c_str(), static_cast<UINT32>(txt.size()), fmt, avail_w, 4096.0f,
            &layout)) &&
        layout) {
      DWRITE_TEXT_METRICS tm{};
      layout->GetMetrics(&tm);
      rt->DrawTextLayout(D2D1::Point2F(text_x + x_off, cur_y), layout, br);
      cur_y += tm.height + gap_after;
      layout->Release();
    }
  };

  draw_wrapped(win->card_title, card_title_fmt, ink_b, 0.0f, 12.0f * scale);

  const std::vector<std::wstring> lines = SplitLines(win->card_body);
  for (const auto& raw_line_txt : lines) {
    if (raw_line_txt.empty()) {
      cur_y += 8.0f * scale;
      continue;
    }
    if (raw_line_txt.rfind(L"# ", 0) == 0 || raw_line_txt.rfind(L"## ", 0) == 0) {
      const size_t strip = raw_line_txt.find(L' ') + 1;
      draw_wrapped(raw_line_txt.substr(strip), h2_fmt, ink_b, 0.0f, 6.0f * scale);
    } else if (raw_line_txt.rfind(L"- ", 0) == 0 ||
               raw_line_txt.rfind(L"* ", 0) == 0) {
      if (accent_b) {
        rt->FillEllipse(
            D2D1::Ellipse(D2D1::Point2F(text_x + 5.0f * scale, cur_y + 9.0f * scale),
                          2.5f * scale, 2.5f * scale),
            accent_b);
      }
      draw_wrapped(raw_line_txt.substr(2), body_fmt, ink_b, 14.0f * scale,
                   5.0f * scale);
    } else {
      draw_wrapped(raw_line_txt, body_fmt, ink_b, 0.0f, 6.0f * scale);
    }
  }

  const std::vector<std::wstring> todos = SplitLines(win->card_todos);
  bool has_todos = false;
  for (const auto& td : todos) {
    if (!td.empty()) {
      has_todos = true;
      break;
    }
  }
  if (has_todos) {
    cur_y += 8.0f * scale;
    if (line_b) {
      rt->DrawLine(D2D1::Point2F(text_x, cur_y),
                   D2D1::Point2F(text_x + text_w, cur_y), line_b,
                   max(1.0f, 1.0f * scale));
    }
    cur_y += 10.0f * scale;
    draw_wrapped(L"待办清单", label_fmt, accent_b, 0.0f, 6.0f * scale);
    for (const auto& td : todos) {
      if (td.empty()) continue;
      if (accent_b) {
        rt->DrawRoundedRectangle(
            D2D1::RoundedRect(
                D2D1::RectF(text_x + 1.0f * scale, cur_y + 4.0f * scale,
                            text_x + 11.0f * scale, cur_y + 14.0f * scale),
                2.5f * scale, 2.5f * scale),
            accent_b, max(1.1f, 1.1f * scale));
      }
      draw_wrapped(td, body_fmt, ink_b, 18.0f * scale, 6.0f * scale);
    }
  }

  const float total_h = (cur_y + win->preview_scroll) - start_content_y;
  const float view_h = max(40.0f, card_bot - start_content_y - 12.0f * scale);
  win->preview_max_scroll = max(0.0f, total_h - view_h);
  if (win->preview_scroll > win->preview_max_scroll) {
    win->preview_scroll = win->preview_max_scroll;
  }

  // Subtle edge fades inside clip_rect to cue scrollability without any right-side scrollbar.
  const D2D1_COLOR_F card_fade =
      LerpD2D(surface, bg, win->dark ? 0.30f : 0.14f, 0.96f);
  const D2D1_COLOR_F card_fade_zero{card_fade.r, card_fade.g, card_fade.b, 0.0f};
  auto draw_fade = [&](float y0, float y1, D2D1_COLOR_F c0, D2D1_COLOR_F c1) {
    D2D1_GRADIENT_STOP stops[2] = {{0.0f, c0}, {1.0f, c1}};
    ID2D1GradientStopCollection* coll = nullptr;
    if (SUCCEEDED(rt->CreateGradientStopCollection(stops, 2, &coll)) && coll) {
      ID2D1LinearGradientBrush* grad = nullptr;
      if (SUCCEEDED(rt->CreateLinearGradientBrush(
              D2D1::LinearGradientBrushProperties(D2D1::Point2F(0.0f, y0),
                                                  D2D1::Point2F(0.0f, y1)),
              coll, &grad)) &&
          grad) {
        rt->FillRectangle(
            D2D1::RectF(clip_rect.left, y0, clip_rect.right, y1), grad);
        grad->Release();
      }
      coll->Release();
    }
  };
  if (win->preview_scroll > 2.0f) {
    draw_fade(clip_rect.top, clip_rect.top + 16.0f * scale, card_fade,
              card_fade_zero);
  }
  if (win->preview_scroll < win->preview_max_scroll - 2.0f) {
    draw_fade(clip_rect.bottom - 22.0f * scale, clip_rect.bottom,
              card_fade_zero, card_fade);
  }

  rt->PopAxisAlignedClip();

  // Footer divider + actions
  if (line_b) {
    rt->DrawLine(D2D1::Point2F(pad, btm_y - 10.0f * scale),
                 D2D1::Point2F(fw - pad, btm_y - 10.0f * scale), line_b,
                 max(1.0f, 1.0f * scale));
  }
  const float edit_btn_w = 92.0f * scale;
  const float back_btn_w = 96.0f * scale;
  draw_btn(D2D1::RectF(fw - pad - edit_btn_w, btm_y, fw - pad, btm_y + btm_h),
           L"编辑卡片", "togglePreview", "", true, false);
  draw_btn(D2D1::RectF(fw - pad - edit_btn_w - 8.0f * scale - back_btn_w, btm_y,
                       fw - pad - edit_btn_w - 8.0f * scale, btm_y + btm_h),
           L"回到主窗体", "dock", "", false, false);

  DrawResizeGrip(rt, fw, fh, scale, muted);

  SafeRelease(btn_fmt);
  SafeRelease(label_fmt);
  SafeRelease(body_fmt);
  SafeRelease(h2_fmt);
  SafeRelease(card_title_fmt);
  SafeRelease(hdr_fmt);
  SafeRelease(eyebrow_fmt);
  SafeRelease(line_b);
  SafeRelease(on_accent_b);
  SafeRelease(accent_b);
  SafeRelease(muted_b);
  SafeRelease(ink_b);
}

void DetachedWidgets::PaintEditorContent(ID2D1RenderTarget* rt,
                                         DetachedWindow* win, int w, int h,
                                         float scale) {
  PaintGlassBackdrop(rt, win, w, h, scale);

  const D2D1_COLOR_F surface = ArgbToD2D(win->surface_argb, 1.0f);
  const D2D1_COLOR_F bg = ArgbToD2D(win->bg_argb, 1.0f);
  const D2D1_COLOR_F ink = ArgbToD2D(win->ink_argb, 1.0f);
  const D2D1_COLOR_F muted = ArgbToD2D(win->muted_argb, 1.0f);
  const D2D1_COLOR_F accent = ArgbToD2D(win->accent_argb, 1.0f);
  const D2D1_COLOR_F on_accent = ArgbToD2D(win->on_accent_argb, 1.0f);
  const D2D1_COLOR_F raw_line = ArgbToD2D(win->line_argb);
  const D2D1_COLOR_F line{raw_line.r, raw_line.g, raw_line.b,
                          max(win->dark ? 0.14f : 0.08f, raw_line.a)};

  const float fw = static_cast<float>(w);
  const float pad = 20.0f * scale;
  const float r_pill = ScaleRadius(10.0f, win->corner_radius, scale);
  const float r_box = ScaleRadius(12.0f, win->corner_radius, scale);

  ID2D1SolidColorBrush* ink_b = nullptr;
  ID2D1SolidColorBrush* muted_b = nullptr;
  ID2D1SolidColorBrush* accent_b = nullptr;
  ID2D1SolidColorBrush* on_accent_b = nullptr;
  ID2D1SolidColorBrush* line_b = nullptr;
  ID2D1SolidColorBrush* field_b = nullptr;
  rt->CreateSolidColorBrush(ink, &ink_b);
  rt->CreateSolidColorBrush(muted, &muted_b);
  rt->CreateSolidColorBrush(accent, &accent_b);
  rt->CreateSolidColorBrush(on_accent, &on_accent_b);
  rt->CreateSolidColorBrush(line, &line_b);
  rt->CreateSolidColorBrush(
      LerpD2D(surface, bg, win->dark ? 0.35f : 0.12f, 1.0f), &field_b);

  IDWriteTextFormat* eyebrow_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 9.0f * scale, DWRITE_FONT_WEIGHT_NORMAL,
      true);
  IDWriteTextFormat* title_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 15.5f * scale,
      DWRITE_FONT_WEIGHT_SEMI_BOLD, true);
  IDWriteTextFormat* label_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 11.5f * scale,
      DWRITE_FONT_WEIGHT_SEMI_BOLD, true);
  IDWriteTextFormat* btn_fmt = CreateTextFormat(
      dwrite_factory_, win->font_family, 11.5f * scale,
      DWRITE_FONT_WEIGHT_SEMI_BOLD, true, DWRITE_TEXT_ALIGNMENT_CENTER,
      DWRITE_PARAGRAPH_ALIGNMENT_CENTER);

  // Header icon + eyebrow + title
  const float badge_sz = 34.0f * scale;
  const D2D1_RECT_F badge_rect =
      D2D1::RectF(pad, 14.0f * scale, pad + badge_sz, 14.0f * scale + badge_sz);
  ID2D1SolidColorBrush* badge_bg = nullptr;
  if (SUCCEEDED(rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, 0.14f), &badge_bg)) &&
      badge_bg) {
    const float br = ScaleRadius(11.0f, win->corner_radius, scale);
    rt->FillRoundedRectangle(D2D1::RoundedRect(badge_rect, br, br), badge_bg);
    badge_bg->Release();
  }
  DrawD2DKindIcon(rt, "editor", badge_rect, accent);

  const float title_x = pad + badge_sz + 12.0f * scale;
  const float title_right = fw - pad - 172.0f * scale;
  if (eyebrow_fmt && muted_b) {
    rt->DrawText(L"MORROW / A LITTLE POSSIBILITY", 27, eyebrow_fmt,
                 D2D1::RectF(title_x, 12.5f * scale, max(title_x + 40.0f, title_right),
                             26.5f * scale),
                 muted_b);
  }
  if (title_fmt && ink_b) {
    rt->DrawText(win->title.c_str(), static_cast<UINT32>(win->title.size()),
                 title_fmt,
                 D2D1::RectF(title_x, 26.5f * scale, max(title_x + 40.0f, title_right),
                             51.0f * scale),
                 ink_b);
  }

  win->buttons.clear();
  auto draw_btn = [&](D2D1_RECT_F rect, const wchar_t* label,
                      const std::string& action, const std::string& value,
                      bool primary, bool highlighted) {
    const bool hovered =
        (win->hovered_action == action && win->hovered_value == value);
    if (primary && accent_b) {
      rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), accent_b);
    } else if (highlighted) {
      ID2D1SolidColorBrush* fb = nullptr;
      ID2D1SolidColorBrush* ab = nullptr;
      rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, hovered ? 0.22f : 0.15f), &fb);
      rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, 0.45f), &ab);
      if (fb) {
        rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
        fb->Release();
      }
      if (ab) {
        rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), ab,
                                 max(1.0f, 1.0f * scale));
        ab->Release();
      }
    } else {
      const D2D1_COLOR_F f = LerpD2D(surface, bg, hovered ? 0.12f : 0.24f,
                                     hovered ? 0.92f : (win->dark ? 0.55f : 0.76f));
      ID2D1SolidColorBrush* fb = nullptr;
      if (SUCCEEDED(rt->CreateSolidColorBrush(f, &fb)) && fb) {
        rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
        fb->Release();
      }
      if (line_b) {
        rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill),
                                 line_b, max(1.0f, 1.0f * scale));
      }
    }
    ID2D1SolidColorBrush* tb =
        primary ? on_accent_b : (highlighted ? accent_b : ink_b);
    if (btn_fmt && tb) {
      rt->DrawText(label, static_cast<UINT32>(wcslen(label)), btn_fmt, rect, tb);
    }
    win->buttons.push_back(ActionRect{
        RECT{static_cast<LONG>(rect.left), static_cast<LONG>(rect.top),
             static_cast<LONG>(rect.right), static_cast<LONG>(rect.bottom)},
        action,
        value});
  };
  auto draw_hdr_icon = [&](D2D1_RECT_F rect, const std::string& icon_kind,
                           const std::string& action, bool highlighted,
                           bool filled_icon) {
    const bool hovered = (win->hovered_action == action);
    if (highlighted) {
      ID2D1SolidColorBrush* fb = nullptr;
      ID2D1SolidColorBrush* ab = nullptr;
      rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, hovered ? 0.22f : 0.15f), &fb);
      rt->CreateSolidColorBrush(
          D2D1::ColorF(accent.r, accent.g, accent.b, 0.45f), &ab);
      if (fb) {
        rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
        fb->Release();
      }
      if (ab) {
        rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), ab,
                                 max(1.0f, 1.0f * scale));
        ab->Release();
      }
    } else {
      const D2D1_COLOR_F f = LerpD2D(surface, bg, hovered ? 0.12f : 0.24f,
                                     hovered ? 0.92f : (win->dark ? 0.50f : 0.72f));
      ID2D1SolidColorBrush* fb = nullptr;
      if (SUCCEEDED(rt->CreateSolidColorBrush(f, &fb)) && fb) {
        rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
        fb->Release();
      }
      if (line_b) {
        rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill),
                                 line_b, max(1.0f, 1.0f * scale));
      }
    }
    DrawD2DActionIcon(rt, icon_kind, rect, highlighted ? accent : muted,
                      filled_icon);
    win->buttons.push_back(ActionRect{
        RECT{static_cast<LONG>(rect.left), static_cast<LONG>(rect.top),
             static_cast<LONG>(rect.right), static_cast<LONG>(rect.bottom)},
        action,
        ""});
  };
  auto draw_hdr_mode_pill = [&](D2D1_RECT_F rect, const std::string& icon_kind,
                                const wchar_t* label) {
    const bool hovered = (win->hovered_action == "togglePreview");
    const D2D1_COLOR_F f = LerpD2D(surface, bg, hovered ? 0.12f : 0.24f,
                                   hovered ? 0.92f : (win->dark ? 0.55f : 0.76f));
    ID2D1SolidColorBrush* fb = nullptr;
    if (SUCCEEDED(rt->CreateSolidColorBrush(f, &fb)) && fb) {
      rt->FillRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), fb);
      fb->Release();
    }
    if (line_b) {
      rt->DrawRoundedRectangle(D2D1::RoundedRect(rect, r_pill, r_pill), line_b,
                               max(1.0f, 1.0f * scale));
    }
    DrawD2DActionIcon(
        rt, icon_kind,
        D2D1::RectF(rect.left + 5.0f * scale, rect.top + 3.0f * scale,
                    rect.left + 23.0f * scale, rect.bottom - 3.0f * scale),
        ink, false);
    if (btn_fmt && ink_b) {
      rt->DrawText(
          label, static_cast<UINT32>(wcslen(label)), btn_fmt,
          D2D1::RectF(rect.left + 20.0f * scale, rect.top, rect.right - 4.0f * scale,
                      rect.bottom),
          ink_b);
    }
    win->buttons.push_back(ActionRect{
        RECT{static_cast<LONG>(rect.left), static_cast<LONG>(rect.top),
             static_cast<LONG>(rect.right), static_cast<LONG>(rect.bottom)},
        "togglePreview",
        ""});
  };

  // Top bar: [Preview pill] [Pin icon] [Dock icon] [Close icon]
  const float top_sz = 28.0f * scale;
  const float top_y = 17.0f * scale;
  const float prev_w = 58.0f * scale;
  const float gap = 6.0f * scale;
  const float close_x = fw - pad - top_sz;
  const float dock_x = close_x - gap - top_sz;
  const float pin_x = dock_x - gap - top_sz;
  const float prev_x = pin_x - gap - prev_w;
  draw_hdr_icon(D2D1::RectF(close_x, top_y, close_x + top_sz, top_y + top_sz),
                "close", "close", false, false);
  draw_hdr_icon(D2D1::RectF(dock_x, top_y, dock_x + top_sz, top_y + top_sz),
                "dock", "dock", false, false);
  draw_hdr_icon(D2D1::RectF(pin_x, top_y, pin_x + top_sz, top_y + top_sz),
                "pin", "togglePin", win->pinned, win->pinned);
  draw_hdr_mode_pill(D2D1::RectF(prev_x, top_y, prev_x + prev_w, top_y + top_sz),
                     "preview", L"预览");

  // Category chips
  struct CatOpt {
    const char* value;
    const wchar_t* label;
  };
  const CatOpt cats[] = {
      {"灵感", L"灵感"},
      {"进行中", L"进行中"},
      {"实验", L"实验"},
  };
  float chip_x = pad;
  const float chip_y = 62.0f * scale;
  const float chip_h = 24.0f * scale;
  for (const auto& c : cats) {
    const float cw = 64.0f * scale;
    draw_btn(D2D1::RectF(chip_x, chip_y, chip_x + cw, chip_y + chip_h), c.label,
             "category", c.value, false, win->category == c.value);
    chip_x += cw + 8.0f * scale;
  }

  // Exact field boxes matching child EDIT HWND geometry
  const int dpi = win->hwnd ? static_cast<int>(GetDpiForWindow(win->hwnd)) : 96;
  const EditorLayout layout = ComputeEditorLayout(w, h, dpi);
  auto to_d2d_rect = [](const RECT& r) {
    return D2D1::RectF(static_cast<float>(r.left), static_cast<float>(r.top),
                       static_cast<float>(r.right), static_cast<float>(r.bottom));
  };
  auto draw_field_box = [&](const RECT& r, bool focused) {
    const D2D1_ROUNDED_RECT rr = D2D1::RoundedRect(to_d2d_rect(r), r_box, r_box);
    if (field_b) rt->FillRoundedRectangle(rr, field_b);
    if (focused) {
      ID2D1SolidColorBrush* focus_b = nullptr;
      if (SUCCEEDED(rt->CreateSolidColorBrush(
              D2D1::ColorF(accent.r, accent.g, accent.b, 0.78f), &focus_b)) &&
          focus_b) {
        rt->DrawRoundedRectangle(rr, focus_b, max(1.6f, 1.6f * scale));
        focus_b->Release();
      }
    } else if (line_b) {
      rt->DrawRoundedRectangle(rr, line_b, max(1.0f, 1.0f * scale));
    }
  };

  draw_field_box(layout.title_box,
                 win->title_edit && win->focused_edit == win->title_edit);
  if (label_fmt && muted_b) {
    const wchar_t* b_lbl = L"正文内容（支持 Markdown）";
    rt->DrawText(b_lbl, static_cast<UINT32>(wcslen(b_lbl)), label_fmt,
                 to_d2d_rect(layout.body_label), muted_b);
  }
  draw_field_box(layout.body_box,
                 win->body_edit && win->focused_edit == win->body_edit);

  if (label_fmt && muted_b) {
    const wchar_t* t_lbl = L"待办清单（每行一条）";
    rt->DrawText(t_lbl, static_cast<UINT32>(wcslen(t_lbl)), label_fmt,
                 to_d2d_rect(layout.todos_label), muted_b);
  }
  draw_field_box(layout.todos_box,
                 win->todos_edit && win->focused_edit == win->todos_edit);

  const float fh = static_cast<float>(h);
  const float btm_h = static_cast<float>(layout.btm_h);
  const float btm_y = static_cast<float>(layout.btm_y);
  if (line_b) {
    rt->DrawLine(D2D1::Point2F(pad, btm_y - 10.0f * scale),
                 D2D1::Point2F(fw - pad, btm_y - 10.0f * scale), line_b,
                 max(1.0f, 1.0f * scale));
  }
  const float save_w = 90.0f * scale;
  const float cancel_w = 72.0f * scale;
  draw_btn(D2D1::RectF(fw - pad - save_w, btm_y, fw - pad, btm_y + btm_h),
           L"保存卡片", "save", "", true, false);
  draw_btn(D2D1::RectF(fw - pad - save_w - 8.0f * scale - cancel_w, btm_y,
                       fw - pad - save_w - 8.0f * scale, btm_y + btm_h),
           L"取消", "close", "", false, false);

  DrawResizeGrip(rt, fw, fh, scale, muted);

  SafeRelease(btn_fmt);
  SafeRelease(label_fmt);
  SafeRelease(title_fmt);
  SafeRelease(eyebrow_fmt);
  SafeRelease(field_b);
  SafeRelease(line_b);
  SafeRelease(on_accent_b);
  SafeRelease(accent_b);
  SafeRelease(muted_b);
  SafeRelease(ink_b);
}

LRESULT CALLBACK DetachedWidgets::WndProc(HWND hwnd, UINT message,
                                          WPARAM wparam, LPARAM lparam) {
  DetachedWindow* win =
      reinterpret_cast<DetachedWindow*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
  if (message == WM_NCCREATE) {
    const auto* cs = reinterpret_cast<CREATESTRUCTW*>(lparam);
    win = reinterpret_cast<DetachedWindow*>(cs->lpCreateParams);
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(win));
    return DefWindowProcW(hwnd, message, wparam, lparam);
  }
  if (!win) return DefWindowProcW(hwnd, message, wparam, lparam);

  switch (message) {
    case WM_NCCALCSIZE:
      if (wparam == TRUE) return 0;
      break;
    case WM_ERASEBKGND:
      return 1;
    case WM_GETMINMAXINFO: {
      auto* mmi = reinterpret_cast<MINMAXINFO*>(lparam);
      const float scale = static_cast<float>(GetDpiForWindow(hwnd)) / 96.0f;
      if (win->kind == "editor") {
        mmi->ptMinTrackSize.x = static_cast<LONG>(420.0f * scale);
        mmi->ptMinTrackSize.y = static_cast<LONG>(360.0f * scale);
        mmi->ptMaxTrackSize.x = static_cast<LONG>(1600.0f * scale);
        mmi->ptMaxTrackSize.y = static_cast<LONG>(1100.0f * scale);
      } else {
        mmi->ptMinTrackSize.x = static_cast<LONG>(240.0f * scale);
        mmi->ptMinTrackSize.y = static_cast<LONG>(150.0f * scale);
        mmi->ptMaxTrackSize.x = static_cast<LONG>(760.0f * scale);
        mmi->ptMaxTrackSize.y = static_cast<LONG>(640.0f * scale);
      }
      return 0;
    }
    case WM_SIZE:
      if (win->owner && win->kind == "editor") {
        win->owner->LayoutEditorControls(win);
      }
      RedrawWindow(hwnd, nullptr, nullptr,
                   RDW_INVALIDATE | RDW_UPDATENOW | RDW_ALLCHILDREN);
      return 0;
    case WM_DPICHANGED: {
      const auto* suggested = reinterpret_cast<const RECT*>(lparam);
      if (suggested) {
        SetWindowPos(hwnd, nullptr, suggested->left, suggested->top,
                     suggested->right - suggested->left,
                     suggested->bottom - suggested->top,
                     SWP_NOZORDER | SWP_NOACTIVATE);
      }
      if (win->owner && win->kind == "editor") {
        win->owner->RefreshEditorFontsAndTheme(win);
        win->owner->LayoutEditorControls(win);
      }
      RedrawWindow(hwnd, nullptr, nullptr,
                   RDW_INVALIDATE | RDW_UPDATENOW | RDW_ALLCHILDREN);
      return 0;
    }
    case WM_CTLCOLOREDIT: {
      if (win->kind == "editor" && win->edit_bg_brush) {
        HDC hdc = reinterpret_cast<HDC>(wparam);
        SetTextColor(hdc, win->edit_fg_color);
        SetBkColor(hdc, win->edit_bg_color);
        return reinterpret_cast<LRESULT>(win->edit_bg_brush);
      }
      break;
    }
    case WM_COMMAND: {
      if (win->kind == "editor") {
        const WORD notify = HIWORD(wparam);
        HWND ctrl = reinterpret_cast<HWND>(lparam);
        if (notify == EN_SETFOCUS &&
            (ctrl == win->title_edit || ctrl == win->body_edit ||
             ctrl == win->todos_edit)) {
          win->focused_edit = ctrl;
          InvalidateRect(hwnd, nullptr, FALSE);
          return 0;
        }
        if (notify == EN_KILLFOCUS && win->focused_edit == ctrl) {
          win->focused_edit = nullptr;
          InvalidateRect(hwnd, nullptr, FALSE);
          return 0;
        }
      }
      break;
    }
    case WM_MOUSEWHEEL: {
      if (win->kind == "editor" && win->preview_mode) {
        const short delta = GET_WHEEL_DELTA_WPARAM(wparam);
        const float step =
            -static_cast<float>(delta) / 120.0f * 36.0f;
        win->preview_scroll =
            std::clamp(win->preview_scroll + step, 0.0f, win->preview_max_scroll);
        InvalidateRect(hwnd, nullptr, FALSE);
        return 0;
      }
      if (win->kind == "editor" && !win->preview_mode && win->owner) {
        RECT rc{};
        GetClientRect(hwnd, &rc);
        const int w = static_cast<int>(max(240L, rc.right - rc.left));
        const int h = static_cast<int>(max(240L, rc.bottom - rc.top));
        const int dpi = static_cast<int>(GetDpiForWindow(hwnd));
        const EditorLayout layout = win->owner->ComputeEditorLayout(w, h, dpi);
        POINT pt{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
        ScreenToClient(hwnd, &pt);

        HWND target = nullptr;
        if (win->todos_edit && PtInRect(&layout.todos_box, pt)) {
          target = win->todos_edit;
        } else if (win->body_edit && PtInRect(&layout.body_box, pt)) {
          target = win->body_edit;
        } else if (GetFocus() == win->todos_edit && win->todos_edit) {
          target = win->todos_edit;
        } else {
          target = win->body_edit;
        }
        if (target) {
          const int lines =
              WheelDeltaToScrollLines(GET_WHEEL_DELTA_WPARAM(wparam));
          if (lines != 0) {
            SendMessageW(target, EM_LINESCROLL, 0, static_cast<LPARAM>(lines));
          }
          return 0;
        }
      }
      break;
    }
    case WM_MOUSEMOVE: {
      if (!win->tracking_leave) {
        TRACKMOUSEEVENT tme{sizeof(tme), TME_LEAVE, hwnd, 0};
        if (TrackMouseEvent(&tme)) win->tracking_leave = true;
      }
      POINT pt{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
      std::string next_action;
      std::string next_value;
      for (const auto& btn : win->buttons) {
        if (PtInRect(&btn.rect, pt)) {
          next_action = btn.action;
          next_value = btn.value;
          break;
        }
      }
      if (next_action != win->hovered_action ||
          next_value != win->hovered_value) {
        win->hovered_action = next_action;
        win->hovered_value = next_value;
        InvalidateRect(hwnd, nullptr, FALSE);
      }
      return 0;
    }
    case WM_MOUSELEAVE:
      win->tracking_leave = false;
      if (!win->hovered_action.empty() || !win->hovered_value.empty()) {
        win->hovered_action.clear();
        win->hovered_value.clear();
        InvalidateRect(hwnd, nullptr, FALSE);
      }
      return 0;
    case WM_NCHITTEST: {
      POINT pt{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
      ScreenToClient(hwnd, &pt);
      RECT rc{};
      GetClientRect(hwnd, &rc);
      const float scale = static_cast<float>(GetDpiForWindow(hwnd)) / 96.0f;
      const int border = max(6, static_cast<int>(kBaseResizeBorder * scale));
      const bool left = pt.x < border;
      const bool right = pt.x >= rc.right - border;
      const bool top = pt.y < border;
      const bool bottom = pt.y >= rc.bottom - border;
      if (top && left) return HTTOPLEFT;
      if (top && right) return HTTOPRIGHT;
      if (bottom && left) return HTBOTTOMLEFT;
      if (bottom && right) return HTBOTTOMRIGHT;
      if (left) return HTLEFT;
      if (right) return HTRIGHT;
      if (top) return HTTOP;
      if (bottom) return HTBOTTOM;

      for (const auto& btn : win->buttons) {
        if (PtInRect(&btn.rect, pt)) return HTCLIENT;
      }
      if (win->kind == "editor" && win->preview_mode && pt.y > 58 * scale) {
        // Allow mouse-wheel events anywhere inside preview card body.
        return HTCLIENT;
      }
      if (win->kind == "editor" && !win->preview_mode && win->owner) {
        const int w = static_cast<int>(max(240L, rc.right - rc.left));
        const int h = static_cast<int>(max(240L, rc.bottom - rc.top));
        const int dpi = static_cast<int>(GetDpiForWindow(hwnd));
        const EditorLayout layout = win->owner->ComputeEditorLayout(w, h, dpi);
        if (PtInRect(&layout.body_box, pt) || PtInRect(&layout.todos_box, pt)) {
          return HTCLIENT;
        }
      }
      return HTCAPTION;
    }
    case WM_LBUTTONUP: {
      POINT pt{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
      for (const auto& btn : win->buttons) {
        if (PtInRect(&btn.rect, pt)) {
          if (win->kind == "editor") {
            if (btn.action == "togglePreview" && win->owner) {
              win->owner->ToggleEditorPreview(win);
            } else if (btn.action == "category" && !btn.value.empty()) {
              win->category = btn.value;
              InvalidateRect(hwnd, nullptr, FALSE);
            } else if (win->owner) {
              win->owner->EmitEditorAction(win, btn.action);
            }
          } else if (win->owner) {
            const std::string id = win->id;
            const std::string action = btn.action;
            win->owner->EmitAction(id, action);
          }
          return 0;
        }
      }
      return 0;
    }
    case WM_PAINT:
      if (win->owner) {
        win->owner->PaintWindow(win);
        return 0;
      }
      break;
    case WM_CLOSE:
      if (win->owner) {
        if (win->kind == "editor") {
          win->owner->EmitEditorAction(win, "close");
        } else {
          win->owner->EmitAction(win->id, "dock");
        }
      }
      DestroyWindow(hwnd);
      return 0;
    case WM_NCDESTROY:
      SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
      if (win->owner) {
        win->owner->DestroyWindowResources(win);
      }
      win->hwnd = nullptr;
      win->title_edit = nullptr;
      win->body_edit = nullptr;
      win->todos_edit = nullptr;
      return 0;
  }
  return DefWindowProcW(hwnd, message, wparam, lparam);
}

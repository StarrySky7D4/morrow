#pragma once
#include <windows.h>
#include <flutter/encodable_value.h>
#include <cmath>
// Qualification only: sample a small owned scene, never arbitrary desktop
// coordinates. This fixture is available only with --canvas-check=.
class CanvasProbe {
 public:
  ~CanvasProbe() { if (fixture_) DestroyWindow(fixture_); }
  bool Start(HWND app) {
    if (!wcsstr(GetCommandLineW(), L"--canvas-check=")) return false;
    app_ = app;
    WNDCLASSW wc{}; wc.lpfnWndProc = Paint; wc.hInstance = GetModuleHandleW(nullptr);
    wc.lpszClassName = L"MorrowOwnedCanvasFixture";
    RegisterClassW(&wc);
    RECT r{}; GetClientRect(app, &r); POINT origin{}; ClientToScreen(app, &origin);
    fixture_ = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, wc.lpszClassName,
      L"Morrow canvas qualification", WS_POPUP, origin.x, origin.y,
      r.right, r.bottom, nullptr, nullptr, wc.hInstance, nullptr);
    if (!fixture_) return false;
    // Qualification only: keep this short-lived owned scene visible without
    // activating it. Normal application windows never become topmost here.
    SetWindowPos(app, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
    SetWindowPos(fixture_, app, origin.x, origin.y, r.right, r.bottom, SWP_SHOWWINDOW | SWP_NOACTIVATE);
    UpdateWindow(fixture_);
    return true;
  }
  flutter::EncodableList Sample() {
    flutter::EncodableList pixels;
    if (!fixture_ || !IsWindow(fixture_)) return pixels;
    POINT origin{}; ClientToScreen(app_, &origin);
    RECT scene{}; GetWindowRect(fixture_, &scene);
    const double scale = GetDpiForWindow(app_) / 96.0;
    HDC dc = GetDC(nullptr);
    // Canvas row, caption background row, then one opaque foreground pixel.
    for (int row : {220, 16, 96}) {
      for (int x = 72; x < (row == 96 ? 73 : 320); x += 8) {
        POINT point{origin.x + static_cast<LONG>(std::lround(x * scale)),
          origin.y + static_cast<LONG>(std::lround(row * scale))};
        const HWND at = GetAncestor(WindowFromPoint(point), GA_ROOT);
        if (!PtInRect(&scene, point) || (at != app_ && at != fixture_)) {
          pixels.clear(); ReleaseDC(nullptr, dc); return pixels;
        }
        const auto color = GetPixel(dc, point.x, point.y);
        pixels.emplace_back(static_cast<int32_t>((GetRValue(color) << 16) |
            (GetGValue(color) << 8) | GetBValue(color)));
      }
    }
    ReleaseDC(nullptr, dc);
    return pixels;
  }
 private:
  HWND fixture_ = nullptr, app_ = nullptr;
  static LRESULT CALLBACK Paint(HWND window, UINT message, WPARAM w, LPARAM l) {
    if (message == WM_ERASEBKGND) return 1;
    if (message == WM_PAINT) {
      PAINTSTRUCT ps{}; HDC dc = BeginPaint(window, &ps); RECT r{}; GetClientRect(window, &r);
      const double scale = GetDpiForWindow(window) / 96.0;
      HBRUSH a = CreateSolidBrush(RGB(240, 50, 80)), b = CreateSolidBrush(RGB(30, 150, 230));
      for (int i = 0; i * 16 * scale < r.right; i++) {
        RECT stripe{static_cast<LONG>(std::lround(i * 16 * scale)), 0,
          static_cast<LONG>(std::lround((i + 1) * 16 * scale)), r.bottom};
        FillRect(dc, &stripe, i % 2 == 0 ? a : b);
      }
      DeleteObject(a); DeleteObject(b); EndPaint(window, &ps); return 0;
    }
    return DefWindowProcW(window, message, w, l);
  }
};

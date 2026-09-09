#include "flutter_window.h"

#include <optional>
#include <algorithm>
#include <cmath>
#include <dwmapi.h>
#include <flutter/standard_method_codec.h>

#include "flutter/generated_plugin_registrant.h"

FlutterWindow::FlutterWindow(const flutter::DartProject& project)
    : project_(project) {}

FlutterWindow::~FlutterWindow() {}

bool FlutterWindow::OnCreate() {
  if (!Win32Window::OnCreate()) {
    return false;
  }

  RECT frame = GetClientArea();

  // The size here must match the window dimensions to avoid unnecessary surface
  // creation / destruction in the startup path.
  flutter_controller_ = std::make_unique<flutter::FlutterViewController>(
      frame.right - frame.left, frame.bottom - frame.top, project_);
  // Ensure that basic setup of the controller was successful.
  if (!flutter_controller_->engine() || !flutter_controller_->view()) {
    return false;
  }
  RegisterPlugins(flutter_controller_->engine());
  SetChildContent(flutter_controller_->view()->GetNativeWindow());
  shape_channel_ = std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
      flutter_controller_->engine()->messenger(), "morrow/window_shape",
      &flutter::StandardMethodCodec::GetInstance());
  shape_channel_->SetMethodCallHandler(
      [this](const auto& call, auto result) {
        if (call.method_name() == "inspectRegion") {
          // Read-only diagnostics used by the native window regression test.
          RECT frame, client;
          POINT origin = {0, 0};
          GetWindowRect(GetHandle(), &frame);
          GetClientRect(GetHandle(), &client);
          ClientToScreen(GetHandle(), &origin);
          const int x = origin.x - frame.left;
          const int y = origin.y - frame.top;
          HRGN region = CreateRectRgn(0, 0, 0, 0);
          const int type = GetWindowRgn(GetHandle(), region);
          flutter::EncodableList corners;
          for (const POINT point : {POINT{x, y}, POINT{x + client.right - 1, y},
               POINT{x, y + client.bottom - 1}, POINT{x + client.right - 1, y + client.bottom - 1}}) {
            corners.push_back(flutter::EncodableValue(type == ERROR || PtInRegion(region, point.x, point.y) != 0));
          }
          const bool center = type == ERROR || PtInRegion(region, x + client.right / 2, y + client.bottom / 2) != 0;
          DeleteObject(region);
          result->Success(flutter::EncodableValue(flutter::EncodableMap{
            {flutter::EncodableValue("cornersVisible"), flutter::EncodableValue(corners)},
            {flutter::EncodableValue("centerVisible"), flutter::EncodableValue(center)},
            {flutter::EncodableValue("maximized"), flutter::EncodableValue(IsZoomed(GetHandle()) != 0)}}));
          return;
        }
        if (call.method_name() != "setRadius") {
          result->NotImplemented();
          return;
        }
        const auto* radius = call.arguments()
            ? std::get_if<double>(call.arguments()) : nullptr;
        if (!radius || !std::isfinite(*radius)) {
          result->Error("invalid_radius", "Radius must be a finite double.");
          return;
        }
        corner_radius_ = std::clamp(*radius, 0.0, 32.0);
        if (ApplyWindowShape()) result->Success();
        else result->Error("window_shape", "Could not update the window region.");
      });

  office_clipboard_ = std::make_unique<OfficeClipboard>(flutter_controller_->engine()->messenger());

  flutter_controller_->engine()->SetNextFrameCallback([&]() {
    this->Show();
  });

  // Flutter can complete the first frame before the "show window" callback is
  // registered. The following call ensures a frame is pending to ensure the
  // window is shown. It is a no-op if the first frame hasn't completed yet.
  flutter_controller_->ForceRedraw();

  return true;
}

void FlutterWindow::OnDestroy() {
  office_clipboard_ = nullptr;
  shape_channel_ = nullptr;
  if (flutter_controller_) {
    flutter_controller_ = nullptr;
  }

  Win32Window::OnDestroy();
}

bool FlutterWindow::ApplyWindowShape() {
  const HWND hwnd = GetHandle();
  if (!hwnd || applying_shape_ || IsIconic(hwnd)) return true;
  applying_shape_ = true;
  // Own the complete outline; otherwise DWM may apply its own corner mask.
  const DWORD corner_preference = 1;  // DWMWCP_DONOTROUND (Windows 11).
  DwmSetWindowAttribute(hwnd, 33, &corner_preference, sizeof(corner_preference));
  bool success = false;
  if (IsZoomed(hwnd) || corner_radius_ == 0) {
    success = SetWindowRgn(hwnd, nullptr, TRUE) != 0;
  } else {
    RECT frame, client;
    POINT origin = {0, 0};
    if (GetWindowRect(hwnd, &frame) && GetClientRect(hwnd, &client) &&
        ClientToScreen(hwnd, &origin)) {
      const int diameter = static_cast<int>(std::lround(
          corner_radius_ * 2 * GetDpiForWindow(hwnd) / 96.0));
      // The Flutter canvas uses client coordinates. SetWindowRgn uses outer
      // window coordinates, which can include invisible resize margins.
      const int left = origin.x - frame.left;
      const int top = origin.y - frame.top;
      HRGN region = CreateRoundRectRgn(left, top, left + client.right + 1,
          top + client.bottom + 1, diameter, diameter);
      if (region) {
        success = SetWindowRgn(hwnd, region, TRUE) != 0;
        // On success Windows takes ownership of this GDI object.
        if (!success) DeleteObject(region);
      }
    }
  }
  applying_shape_ = false;
  return success;
}

LRESULT
FlutterWindow::MessageHandler(HWND hwnd, UINT const message,
                              WPARAM const wparam,
                              LPARAM const lparam) noexcept {
  // Give Flutter, including plugins, an opportunity to handle window messages.
  if (flutter_controller_) {
    std::optional<LRESULT> result =
        flutter_controller_->HandleTopLevelWindowProc(hwnd, message, wparam,
                                                      lparam);
    if (result) {
      if (message == WM_SIZE || message == WM_DPICHANGED ||
          message == WM_WINDOWPOSCHANGED) ApplyWindowShape();
      return *result;
    }
  }

  switch (message) {
    case WM_FONTCHANGE:
      flutter_controller_->engine()->ReloadSystemFonts();
      break;
  }

  const LRESULT result = Win32Window::MessageHandler(hwnd, message, wparam, lparam);
  if (message == WM_SIZE || message == WM_DPICHANGED ||
      message == WM_WINDOWPOSCHANGED) ApplyWindowShape();
  return result;
}

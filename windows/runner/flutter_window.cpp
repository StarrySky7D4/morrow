#include "flutter_window.h"

#include <optional>
#include <algorithm>
#include <cmath>
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
      flutter_controller_->engine()->messenger(), "daemon/window_shape",
      &flutter::StandardMethodCodec::GetInstance());
  shape_channel_->SetMethodCallHandler(
      [this](const auto& call, auto result) {
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
  bool success = false;
  if (IsZoomed(hwnd) || corner_radius_ == 0) {
    success = SetWindowRgn(hwnd, nullptr, TRUE) != 0;
  } else {
    RECT frame;
    if (GetWindowRect(hwnd, &frame)) {
      const int diameter = static_cast<int>(std::lround(
          corner_radius_ * 2 * GetDpiForWindow(hwnd) / 96.0));
      HRGN region = CreateRoundRectRgn(0, 0, frame.right - frame.left + 1,
          frame.bottom - frame.top + 1, diameter, diameter);
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
      if (message == WM_SIZE || message == WM_DPICHANGED) ApplyWindowShape();
      return *result;
    }
  }

  switch (message) {
    case WM_FONTCHANGE:
      flutter_controller_->engine()->ReloadSystemFonts();
      break;
  }

  const LRESULT result = Win32Window::MessageHandler(hwnd, message, wparam, lparam);
  if (message == WM_SIZE || message == WM_DPICHANGED) ApplyWindowShape();
  return result;
}

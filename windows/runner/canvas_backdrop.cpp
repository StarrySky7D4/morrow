#include "canvas_backdrop.h"
#include <algorithm>
#include <cmath>
#include <dwmapi.h>
#include <d2d1effects.h>
#include <DispatcherQueue.h>
#include <windows.graphics.effects.interop.h>
#include <windows.ui.composition.interop.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Graphics.Effects.h>
#include <winrt/Windows.System.h>
#include <winrt/Windows.UI.Composition.h>
#include <winrt/Windows.UI.Composition.Desktop.h>
using namespace winrt;
using namespace Windows::UI::Composition;
using namespace Windows::Graphics::Effects;
namespace {
// Fixed Gaussian effect graph; the only animatable input is a bounded radius.
struct BlurEffect : implements<BlurEffect, IGraphicsEffect, IGraphicsEffectSource,
    ABI::Windows::Graphics::Effects::IGraphicsEffectD2D1Interop> {
  hstring Name() { return L"Frost"; }
  void Name(hstring const&) {}
  IGraphicsEffectSource source{CompositionEffectSourceParameter(L"Desktop")};
  HRESULT __stdcall GetEffectId(GUID* id) noexcept override {
    if (!id) return E_POINTER;
    *id = CLSID_D2D1GaussianBlur; return S_OK;
  }
  HRESULT __stdcall GetNamedPropertyMapping(LPCWSTR name, UINT* index,
      ABI::Windows::Graphics::Effects::GRAPHICS_EFFECT_PROPERTY_MAPPING* mapping) noexcept override {
    if (!name || !index || !mapping) return E_POINTER;
    if (wcscmp(name, L"BlurAmount") != 0) return E_INVALIDARG;
    *index = D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION;
    *mapping = ABI::Windows::Graphics::Effects::GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT; return S_OK;
  }
  HRESULT __stdcall GetPropertyCount(UINT* count) noexcept override {
    if (!count) return E_POINTER; *count = 3; return S_OK;
  }
  HRESULT __stdcall GetProperty(UINT index, ABI::Windows::Foundation::IPropertyValue** value) noexcept override {
    if (!value) return E_POINTER; *value = nullptr;
    try {
      Windows::Foundation::IInspectable result{nullptr};
      using Windows::Foundation::PropertyValue;
      switch (index) {
        case 0: result = PropertyValue::CreateSingle(0); break;
        case 1: result = PropertyValue::CreateUInt32(D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED); break;
        case 2: result = PropertyValue::CreateUInt32(D2D1_BORDER_MODE_HARD); break;
        default: return E_INVALIDARG;
      }
      return result.as<ABI::Windows::Foundation::IPropertyValue>().copy_to(value), S_OK;
    } catch (...) { return to_hresult(); }
  }
  HRESULT __stdcall GetSource(UINT index, ABI::Windows::Graphics::Effects::IGraphicsEffectSource** result) noexcept override {
    if (!result) return E_POINTER; *result = nullptr;
    if (index != 0) return E_INVALIDARG;
    return source.as<ABI::Windows::Graphics::Effects::IGraphicsEffectSource>().copy_to(result), S_OK;
  }
  HRESULT __stdcall GetSourceCount(UINT* count) noexcept override {
    if (!count) return E_POINTER; *count = 1; return S_OK;
  }
};
}
struct CanvasBackdrop::State {
  Windows::System::DispatcherQueueController queue{nullptr};
  Compositor compositor{nullptr};
  Desktop::DesktopWindowTarget target{nullptr};
  SpriteVisual visual{nullptr};
  CompositionRoundedRectangleGeometry shape{nullptr};
  CompositionEffectBrush brush{nullptr};
  HWND window = nullptr;
  double top_inset = 0;
  ~State() {
    try { if (target) { target.Root(nullptr); target.Close(); } } catch (...) {}
    if (window) DestroyWindow(window);
  }
};
CanvasBackdrop::CanvasBackdrop() = default;
CanvasBackdrop::~CanvasBackdrop() = default;
void CanvasBackdrop::Reset() { state_.reset(); }
void CanvasBackdrop::UpdateBounds(HWND app, double corner_radius) {
  if (corner_radius >= 0) corner_radius_ = corner_radius;
  if (!state_ || updating_) return;
  updating_ = true;
  RECT r{}; POINT origin{};
  GetClientRect(app, &r); ClientToScreen(app, &origin);
  const int top = static_cast<int>(std::lround(state_->top_inset * GetDpiForWindow(app) / 96.0));
  if (IsIconic(app) || !IsWindowVisible(app) || r.bottom <= top) {
    ShowWindow(state_->window, SW_HIDE);
  } else {
    // A separate, non-activating background surface; never an owner/foreground
    // window. Insert immediately behind Flutter, including the caption background.
    SetWindowPos(state_->window, app, origin.x, origin.y + top, r.right,
      r.bottom - top, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    // The HRGN is deliberately a loose binary hit-test envelope. The blur
    // needs its own smooth shape or it would fill the transparent AA fringe.
    // Geometry uses the full client canvas, translated when an inset is used.
    try {
      const float radius = IsZoomed(app) ? 0.0f : static_cast<float>(
          std::min(corner_radius_ * GetDpiForWindow(app) / 96.0,
              std::min(r.right, r.bottom) / 2.0));
      state_->shape.Size({static_cast<float>(r.right), static_cast<float>(r.bottom)});
      state_->shape.Offset({0, -static_cast<float>(top)});
      state_->shape.CornerRadius({radius, radius});
    } catch (...) {
      // A failed compositor cannot leave an unbounded blur behind the app.
      state_.reset();
    }

  }
  updating_ = false;
}
bool CanvasBackdrop::Set(HWND app, double blur, double top_inset) {
  try {
    if (blur == 0 && !state_) return true;
    // Keep the transparent visual alive at zero so both directions can fade.
    // The application explicitly releases it when its native window closes.
    if (!state_) {
      auto candidate = std::make_unique<State>();
      if (!Windows::System::DispatcherQueue::GetForCurrentThread()) {
        DispatcherQueueOptions options{sizeof(DispatcherQueueOptions), DQTYPE_THREAD_CURRENT, DQTAT_COM_STA};
        check_hresult(CreateDispatcherQueueController(options,
          reinterpret_cast<ABI::Windows::System::IDispatcherQueueController**>(put_abi(candidate->queue))));
      }
      WNDCLASSW wc{}; wc.hInstance = GetModuleHandleW(nullptr);
      wc.lpszClassName = L"MorrowCanvasBackdrop";
      wc.lpfnWndProc = [](HWND window, UINT message, WPARAM w, LPARAM l) -> LRESULT {
        if (message == WM_NCHITTEST) return HTTRANSPARENT;
        if (message == WM_ERASEBKGND) return 1;
        return DefWindowProcW(window, message, w, l);
      };
      RegisterClassW(&wc);
      candidate->window = CreateWindowExW(WS_EX_NOREDIRECTIONBITMAP | WS_EX_NOACTIVATE |
        WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT, wc.lpszClassName, L"", WS_POPUP,
        0, 0, 1, 1, nullptr, nullptr, wc.hInstance, nullptr);
      if (!candidate->window) throw hresult_error(E_FAIL);
      const BOOL enable = TRUE;
      check_hresult(DwmSetWindowAttribute(candidate->window, 17, &enable, sizeof(enable)));
      candidate->compositor = Compositor();
      auto interop = candidate->compositor.as<ABI::Windows::UI::Composition::Desktop::ICompositorDesktopInterop>();
      check_hresult(interop->CreateDesktopWindowTarget(candidate->window, FALSE,
        reinterpret_cast<ABI::Windows::UI::Composition::Desktop::IDesktopWindowTarget**>(put_abi(candidate->target))));
      auto factory = candidate->compositor.CreateEffectFactory(make<BlurEffect>(), {L"Frost.BlurAmount"});
      candidate->brush = factory.CreateBrush();
      candidate->brush.SetSourceParameter(L"Desktop", candidate->compositor.CreateHostBackdropBrush());
      candidate->visual = candidate->compositor.CreateSpriteVisual();
      candidate->visual.RelativeSizeAdjustment({1, 1});
      candidate->visual.Opacity(0);
      candidate->visual.Brush(candidate->brush);
      candidate->shape = candidate->compositor.CreateRoundedRectangleGeometry();
      candidate->visual.Clip(candidate->compositor.CreateGeometricClip(candidate->shape));
      candidate->target.Root(candidate->visual);
      state_ = std::move(candidate);
    }
    state_->top_inset = top_inset;
    UpdateBounds(app);
    if (!state_) return false;
    const float value = static_cast<float>(std::clamp(blur, 0.0, 40.0));
    // HostBackdropBrush already contains system blur. A radius alone cannot
    // approach the clear desktop at zero: progressively mix in that source over
    // the first 20% of the slider, with a smooth slope at both ends.
    const float mix = std::clamp(value / 8.0f, 0.0f, 1.0f);
    const float opacity = mix * mix * (3.0f - 2.0f * mix);
    BOOL animate = TRUE;
    SystemParametersInfo(SPI_GETCLIENTAREAANIMATION, 0, &animate, 0);
    if (animate) {
      auto animation = state_->compositor.CreateScalarKeyFrameAnimation();
      animation.InsertKeyFrame(1, value);
      animation.Duration(std::chrono::milliseconds(280));
      state_->brush.Properties().StartAnimation(L"Frost.BlurAmount", animation);
      auto fade = state_->compositor.CreateScalarKeyFrameAnimation();
      fade.InsertKeyFrame(1, opacity);
      fade.Duration(std::chrono::milliseconds(280));
      state_->visual.StartAnimation(L"Opacity", fade);
    } else {
      state_->brush.Properties().StopAnimation(L"Frost.BlurAmount");
      state_->brush.Properties().InsertScalar(L"Frost.BlurAmount", value);
      state_->visual.StopAnimation(L"Opacity");
      state_->visual.Opacity(opacity);
    }
    return true;
  } catch (...) {
    state_.reset();
    return false;
  }
}

#ifndef RUNNER_DETACHED_WIDGETS_H_
#define RUNNER_DETACHED_WIDGETS_H_

#include <d2d1.h>
#include <dwrite.h>
#include <flutter/binary_messenger.h>
#include <flutter/encodable_value.h>
#include <flutter/method_channel.h>
#include <windows.h>

#include <cstdint>
#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

class DetachedWidgets {
 public:
  DetachedWidgets(HWND main_window, flutter::BinaryMessenger* messenger);
  ~DetachedWidgets();

 private:
  struct ActionRect {
    RECT rect{};
    std::string action;
    std::string value;
  };

  struct EditorLayout {
    RECT title_box{};
    RECT title_edit{};
    RECT body_label{};
    RECT body_box{};
    RECT body_edit{};
    RECT todos_label{};
    RECT todos_box{};
    RECT todos_edit{};
    int btm_y = 0;
    int btm_h = 0;
    int pad = 0;
  };

  struct DetachedWindow {
    std::string id;
    std::string kind;
    std::wstring title;
    std::wstring primary_text;
    std::wstring secondary_text;
    std::wstring badge_text;
    std::wstring card_title;
    std::wstring card_body;
    std::wstring card_todos;
    std::string category = "灵感";
    double progress = 0.0;
    bool running = false;
    bool pinned = true;
    bool dark = false;
    bool is_solid = false;
    bool preview_mode = false;
    bool tracking_leave = false;
    float frosted_opacity = 0.88f;
    float preview_scroll = 0.0f;
    float preview_max_scroll = 0.0f;
    uint32_t surface_argb = 0xFFFFFFFF;
    uint32_t bg_argb = 0xFFF9F8FC;
    uint32_t ink_argb = 0xFF302D43;
    uint32_t muted_argb = 0xFF777184;
    uint32_t accent_argb = 0xFF7662BA;
    uint32_t on_accent_argb = 0xFFFFFFFF;
    uint32_t line_argb = 0x13302D43;
    uint32_t edge_argb = 0x38302D43;
    float corner_radius = 20.0f;
    std::wstring font_family = L"Microsoft YaHei UI";
    std::string hovered_action;
    std::string hovered_value;
    HWND hwnd = nullptr;
    HWND title_edit = nullptr;
    HWND body_edit = nullptr;
    HWND todos_edit = nullptr;
    HWND focused_edit = nullptr;
    HFONT ui_font = nullptr;
    HFONT title_font = nullptr;
    HBRUSH edit_bg_brush = nullptr;
    COLORREF edit_bg_color = 0;
    COLORREF edit_fg_color = 0;
    DetachedWidgets* owner = nullptr;
    std::vector<ActionRect> buttons;
  };

  static LRESULT CALLBACK WndProc(HWND hwnd, UINT message, WPARAM wparam,
                                  LPARAM lparam);
  void RegisterWindowClass();
  bool EnsureD2DFactories();
  void ReadThemeTokens(const flutter::EncodableMap& args, DetachedWindow* win);
  void OpenOrUpdate(const flutter::EncodableMap& args);
  void OpenEditor(const flutter::EncodableMap& args);
  void CloseById(const std::string& id);
  void CloseAll();
  void EmitAction(const std::string& id, const std::string& action);
  void EmitEditorAction(DetachedWindow* win, const std::string& action);
  void CreateEditorControls(DetachedWindow* win, const std::wstring& init_title,
                            const std::wstring& init_body,
                            const std::wstring& init_todos);
  void ToggleEditorPreview(DetachedWindow* win);
  void SyncEditControlVisibility(DetachedWindow* win);
  EditorLayout ComputeEditorLayout(int w, int h, int dpi) const;
  void LayoutEditorControls(DetachedWindow* win);
  void RefreshEditorFontsAndTheme(DetachedWindow* win);
  void DestroyWindowResources(DetachedWindow* win);
  POINT ComputeInitialPosition(const flutter::EncodableMap& args, int px_w,
                               int px_h);
  void PaintWindow(DetachedWindow* win);
  void PaintGlassBackdrop(ID2D1RenderTarget* rt, DetachedWindow* win, int w,
                          int h, float scale);
  void PaintWidgetContent(ID2D1RenderTarget* rt, DetachedWindow* win, int w,
                          int h, float scale);
  void PaintEditorContent(ID2D1RenderTarget* rt, DetachedWindow* win, int w,
                          int h, float scale);
  void PaintCardPreview(ID2D1RenderTarget* rt, DetachedWindow* win, int w,
                        int h, float scale);
  void ApplyWindowChrome(DetachedWindow* win);
  void UpdateTopmostFlag(DetachedWindow* win);

  HWND main_window_ = nullptr;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>> channel_;
  std::unordered_map<std::string, std::unique_ptr<DetachedWindow>> windows_;
  ID2D1Factory* d2d_factory_ = nullptr;
  ID2D1DCRenderTarget* dc_target_ = nullptr;
  IDWriteFactory* dwrite_factory_ = nullptr;
  bool class_registered_ = false;
};

#endif  // RUNNER_DETACHED_WIDGETS_H_

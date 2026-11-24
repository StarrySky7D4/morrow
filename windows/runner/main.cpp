#include <flutter/dart_project.h>
#include <flutter/flutter_view_controller.h>
#include <windows.h>

#include <cwchar>

#include "flutter_window.h"
#include "utils.h"

// Request the high-performance GPU on hybrid-graphics systems.
extern "C" {
__declspec(dllexport) DWORD NvOptimusEnablement = 0x00000001;
__declspec(dllexport) int AmdPowerXpressRequestHighPerformance = 1;
}

int APIENTRY wWinMain(_In_ HINSTANCE instance, _In_opt_ HINSTANCE prev,
                      _In_ wchar_t *command_line, _In_ int show_command) {
  // Attach to console when present (e.g., 'flutter run') or create a
  // new console when running with a debugger.
  if (!::AttachConsole(ATTACH_PARENT_PROCESS) && ::IsDebuggerPresent()) {
    CreateAndAttachConsole();
  }

  // Initialize COM, so that it is available for use in the library and/or
  // plugins.
  ::CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);

  flutter::DartProject project(L"data");
  project.set_gpu_preference(flutter::GpuPreference::HighPerformancePreference);

  // Flutter 3.50's experimental Windows Impeller (OpenGLESSDF) backend incurs
  // ~550 MB extra UMA/driver RAM on Windows. Default to hardware-accelerated
  // Skia over ANGLE D3D11 on the high-performance GPU, with opt-in Impeller
  // via MORROW_ENABLE_IMPELLER=1.
  wchar_t enable_impeller[8] = {};
  const DWORD env_len = ::GetEnvironmentVariableW(
      L"MORROW_ENABLE_IMPELLER", enable_impeller, 8);
  if (env_len > 0 && wcscmp(enable_impeller, L"1") == 0) {
    project.set_impeller_switch(flutter::ImpellerSwitch::Enabled);
  } else {
    project.set_impeller_switch(flutter::ImpellerSwitch::Disabled);
  }

  std::vector<std::string> command_line_arguments =
      GetCommandLineArguments();

  project.set_dart_entrypoint_arguments(std::move(command_line_arguments));

  FlutterWindow window(project);
  Win32Window::Point origin(10, 10);
  Win32Window::Size size(1280, 720);
  if (!window.Create(L"Morrow", origin, size)) {
    return EXIT_FAILURE;
  }
  window.SetQuitOnClose(true);

  ::MSG msg;
  while (::GetMessage(&msg, nullptr, 0, 0)) {
    ::TranslateMessage(&msg);
    ::DispatchMessage(&msg);
  }

  // window_manager posts WM_QUIT without destroying the HWND. Tear down the
  // live Flutter view while derived handlers and COM are still valid. Letting
  // member destruction do this after CoUninitialize leaves callbacks pointing
  // at a partly destroyed controller during native child-window teardown.
  window.Destroy();
  ::CoUninitialize();
  return EXIT_SUCCESS;
}

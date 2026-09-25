// Read-only MSAA hit testing of one test process during Flutter route changes.
#include <windows.h>
#include <oleacc.h>
#include <shellapi.h>
#include <cstdio>
#include <cstdlib>
static DWORD target;
static HWND view;
BOOL CALLBACK Child(HWND hwnd, LPARAM) {
  wchar_t name[128]{};
  GetClassNameW(hwnd, name, 128);
  if (wcscmp(name, L"FLUTTERVIEW") == 0) view = hwnd;
  return TRUE;
}
BOOL CALLBACK Window(HWND hwnd, LPARAM) {
  DWORD pid = 0;
  GetWindowThreadProcessId(hwnd, &pid);
  if (pid == target) EnumChildWindows(hwnd, Child, 0);
  return TRUE;
}
int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR, int) {
  int argc;
  auto argv = CommandLineToArgvW(GetCommandLineW(), &argc);
  if (argc != 4) return 2;
  target = wcstoul(argv[1], nullptr, 10);
  const DWORD duration = wcstoul(argv[2], nullptr, 10);
  FILE* log = nullptr;
  _wfopen_s(&log, argv[3], L"w");
  LocalFree(argv);
  if (!log) return 3;
  CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
  const auto started = GetTickCount64();
  unsigned queries = 0, hits = 0;
  while (GetTickCount64() - started < duration) {
    if (!view || !IsWindow(view)) {view = nullptr; EnumWindows(Window, 0);}
    if (!view) {Sleep(30); continue;}
    IAccessible* root = nullptr;
    HRESULT hr = AccessibleObjectFromWindow(view, OBJID_CLIENT, IID_IAccessible,
                                           reinterpret_cast<void**>(&root));
    if (SUCCEEDED(hr) && root) {
      RECT r{}; GetWindowRect(view, &r);
      for (int y = 1; y < 10; y++) for (int x = 1; x < 10; x++) {
        VARIANT result; VariantInit(&result);
        hr = root->accHitTest(r.left + (r.right-r.left)*x/10,
                             r.top + (r.bottom-r.top)*y/10, &result);
        queries++; if (SUCCEEDED(hr) && result.vt != VT_EMPTY) hits++;
        VariantClear(&result);
      }
      root->Release();
    }
    Sleep(10);
  }
  fprintf(log, "queries=%u\nhits=%u\n", queries, hits);
  fclose(log);
  CoUninitialize();
  return hits ? 0 : 4;
}

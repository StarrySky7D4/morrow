#define MORROW_CLIPBOARD_TEST
#include "../../windows/runner/office_clipboard.cpp"
#include <iostream>
int main() {
  if (FAILED(OleInitialize(nullptr))) return 1;
  ComPtr<IStream> stream;
  if (FAILED(CreateStreamOnHGlobal(nullptr, TRUE, &stream))) return 2;
  const char payload[] = "fixture";
  ULONG wrote = 0;
  stream->Write(payload, 7, &wrote);
  if (StreamBytes(stream.Get()) != Bytes(payload, payload + 7)) return 3;
  ComPtr<ILockBytes> lock;
  ComPtr<IStorage> storage;
  if (FAILED(CreateILockBytesOnHGlobal(nullptr, TRUE, &lock))) return 4;
  if (FAILED(StgCreateDocfileOnILockBytes(lock.Get(), STGM_CREATE | STGM_READWRITE | STGM_SHARE_EXCLUSIVE, 0, &storage))) return 5;
  ComPtr<IStream> child;
  storage->CreateStream(L"Workbook", STGM_CREATE | STGM_READWRITE | STGM_SHARE_EXCLUSIVE, 0, 0, &child);
  child->Write(payload, 7, &wrote);
  child.Reset();
  size_t total = 0;
  if (!HasStream(storage.Get(), L"Workbook") || !StorageFits(storage.Get(), total) || total != 7) return 6;
  total = kMaxBytes;
  if (StorageFits(storage.Get(), total)) return 7;
  RECT frame{0,0,2540,1270};
  HDC dc = CreateEnhMetaFileW(nullptr, nullptr, &frame, nullptr);
  if (!dc) return 8;
  Rectangle(dc, 0, 0, 96, 48);
  TextOutW(dc, 4, 4, L"Office chart", 12);
  HENHMETAFILE emf = CloseEnhMetaFile(dc);
  Gdiplus::GdiplusStartupInput input;
  ULONG_PTR token = 0;
  if (Gdiplus::GdiplusStartup(&token, &input, nullptr) != Gdiplus::Ok) return 9;
  const Bytes png = RenderMetafile(emf);
  Gdiplus::GdiplusShutdown(token);
  DeleteEnhMetaFile(emf);
  if (png.size() < 8 || png[0] != 137 || png[1] != 80 || png[2] != 78 || png[3] != 71) return 10;
  storage.Reset(); lock.Reset(); stream.Reset();
  OleUninitialize();
  std::cout << "PASS: COM stream, compound Office storage, size guard, EMF to PNG preview\n";
  return 0;
}

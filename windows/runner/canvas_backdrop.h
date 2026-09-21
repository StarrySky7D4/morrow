#pragma once
#include <windows.h>
#include <memory>
// Owns only the window's visual backdrop. Never captures desktop pixels.
class CanvasBackdrop {
 public:
  CanvasBackdrop();
  ~CanvasBackdrop();
  bool Set(HWND window, double blur, double top_inset = 0);
  void UpdateBounds(HWND window, double corner_radius = -1);
  void Reset();
 private:
  bool updating_ = false;
  double corner_radius_ = 20;
  struct State;
  std::unique_ptr<State> state_;
};

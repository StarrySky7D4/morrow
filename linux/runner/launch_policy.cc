#include "launch_policy.h"

#include <cstring>

LinuxLaunchMode ParseLinuxLaunchMode(int argc, char* const argv[]) {
  if (argc != 2 || argv == nullptr || argv[1] == nullptr) {
    return LinuxLaunchMode::kUnavailable;
  }
  if (std::strcmp(argv[1], "--linux-gtk-prerequisite-probe") == 0) {
    return LinuxLaunchMode::kGtkProbe;
  }
  if (std::strcmp(argv[1], "--linux-foundation-status") == 0) {
    return LinuxLaunchMode::kExplain;
  }
  return LinuxLaunchMode::kUnavailable;
}

const char* LinuxFoundationBoundary() {
  return "Morrow Linux foundation: protected Store and product owner wiring "
         "are unavailable. Ordinary launch is disabled. "
         "--linux-gtk-prerequisite-probe opens a diagnostic GTK window only; "
         "it does not start Flutter, open a profile, or create a content library.";
}

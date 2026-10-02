#include "launch_policy.h"

#include <cassert>
#include <cstring>

int main() {
  char name[] = "morrow_studio";
  char probe[] = "--linux-gtk-prerequisite-probe";
  char explain[] = "--linux-foundation-status";
  char product[] = "--profile";
  char* empty[] = {name, nullptr};
  char* valid_probe[] = {name, probe, nullptr};
  char* valid_explain[] = {name, explain, nullptr};
  char* unexpected[] = {name, product, nullptr};
  char* appended[] = {name, probe, product, nullptr};
  assert(ParseLinuxLaunchMode(1, empty) == LinuxLaunchMode::kUnavailable);
  assert(ParseLinuxLaunchMode(2, valid_probe) == LinuxLaunchMode::kGtkProbe);
  assert(ParseLinuxLaunchMode(2, valid_explain) == LinuxLaunchMode::kExplain);
  assert(ParseLinuxLaunchMode(2, unexpected) == LinuxLaunchMode::kUnavailable);
  assert(ParseLinuxLaunchMode(3, appended) == LinuxLaunchMode::kUnavailable);
  assert(ParseLinuxLaunchMode(2, nullptr) == LinuxLaunchMode::kUnavailable);
  assert(std::strstr(LinuxFoundationBoundary(), "Ordinary launch is disabled") != nullptr);
}

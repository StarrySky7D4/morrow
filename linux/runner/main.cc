#include "launch_policy.h"
#include "my_application.h"

#include <cstdio>

int main(int argc, char** argv) {
  const auto mode = ParseLinuxLaunchMode(argc, argv);
  if (mode != LinuxLaunchMode::kGtkProbe) {
    std::fprintf(stderr, "%s\n", LinuxFoundationBoundary());
    return mode == LinuxLaunchMode::kExplain ? 0 : 2;
  }
  g_autoptr(MyApplication) application = my_application_new();
  // The only accepted flag was consumed above. GTK receives just argv[0].
  return g_application_run(G_APPLICATION(application), 1, argv);
}

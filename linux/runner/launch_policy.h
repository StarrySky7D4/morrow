#ifndef MORROW_LINUX_LAUNCH_POLICY_H_
#define MORROW_LINUX_LAUNCH_POLICY_H_

// The foundation runner has no protected Store or product supervisor binding.
// Parse before initializing GTK/Flutter so normal launch cannot fall through
// to Dart's existing non-Windows legacy storage path.
enum class LinuxLaunchMode { kUnavailable, kGtkProbe, kExplain };
LinuxLaunchMode ParseLinuxLaunchMode(int argc, char* const argv[]);
const char* LinuxFoundationBoundary();

#endif

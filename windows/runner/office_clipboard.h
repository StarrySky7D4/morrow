#ifndef RUNNER_OFFICE_CLIPBOARD_H_
#define RUNNER_OFFICE_CLIPBOARD_H_
#include <flutter/binary_messenger.h>
#include <flutter/method_channel.h>
#include <flutter/encodable_value.h>
#include <memory>
struct OfficeClipboardState;
class OfficeClipboard {
 public:
  explicit OfficeClipboard(flutter::BinaryMessenger* messenger);
  ~OfficeClipboard();
 private:
  std::shared_ptr<OfficeClipboardState> state_;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>> channel_;
};
#endif

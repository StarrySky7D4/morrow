#include "system_fonts.h"

#include <windows.h>
#include <dwrite.h>
#include <wrl/client.h>

#include <flutter/standard_method_codec.h>

#include <algorithm>
#include <cctype>
#include <optional>
#include <string>
#include <unordered_set>
#include <vector>

using flutter::EncodableList;
using flutter::EncodableMap;
using flutter::EncodableValue;
using Microsoft::WRL::ComPtr;

namespace {

std::string Utf8FromWide(const std::wstring& wide) {
  if (wide.empty()) return {};
  const int length = static_cast<int>(wide.size());
  const int size = WideCharToMultiByte(
      CP_UTF8, 0, wide.data(), length, nullptr, 0, nullptr, nullptr);
  if (size <= 0) return {};
  std::string result(static_cast<size_t>(size), '\0');
  WideCharToMultiByte(
      CP_UTF8, 0, wide.data(), length, result.data(), size, nullptr, nullptr);
  return result;
}

std::wstring WideFromUtf8(const std::string& utf8) {
  if (utf8.empty()) return {};
  const int length = static_cast<int>(utf8.size());
  const int size = MultiByteToWideChar(
      CP_UTF8, 0, utf8.data(), length, nullptr, 0);
  if (size <= 0) return {};
  std::wstring result(static_cast<size_t>(size), L'\0');
  MultiByteToWideChar(
      CP_UTF8, 0, utf8.data(), length, result.data(), size);
  return result;
}

std::string AsciiLower(std::string value) {
  std::transform(value.begin(), value.end(), value.begin(), [](unsigned char c) {
    return static_cast<char>(std::tolower(c));
  });
  return value;
}

bool StartsWithZh(const std::wstring& locale) {
  if (locale.size() < 2) return false;
  const wchar_t c0 = locale[0];
  const wchar_t c1 = locale[1];
  return (c0 == L'z' || c0 == L'Z') && (c1 == L'h' || c1 == L'H');
}

std::optional<std::string> TrimAndValidateFamily(std::string value) {
  const auto first = value.find_first_not_of(" \t");
  if (first == std::string::npos) return std::nullopt;
  const auto last = value.find_last_not_of(" \t");
  value = value.substr(first, last - first + 1);
  if (value.empty() || value.front() == '@' || value.size() > 128) {
    return std::nullopt;
  }
  for (size_t i = 0; i < value.size(); ++i) {
    const unsigned char byte = static_cast<unsigned char>(value[i]);
    if (byte < 32 || byte == 127) {
      return std::nullopt;
    }
    if (byte == 0xC2 && i + 1 < value.size()) {
      const unsigned char next = static_cast<unsigned char>(value[i + 1]);
      if (next >= 0x80 && next <= 0x9F) {
        return std::nullopt;
      }
    }
  }
  return value;
}

std::optional<std::string> ReadLocalizedName(IDWriteLocalizedStrings* names,
                                             UINT32 index) {
  if (!names) return std::nullopt;
  UINT32 length = 0;
  if (FAILED(names->GetStringLength(index, &length))) {
    return std::nullopt;
  }
  std::wstring buffer(static_cast<size_t>(length) + 1, L'\0');
  if (FAILED(names->GetString(index, buffer.data(), length + 1))) {
    return std::nullopt;
  }
  buffer.resize(static_cast<size_t>(length));
  return Utf8FromWide(buffer);
}

std::optional<std::string> FindLocaleString(IDWriteLocalizedStrings* names,
                                            const std::wstring& locale) {
  if (!names || locale.empty()) return std::nullopt;
  UINT32 index = 0;
  BOOL exists = FALSE;
  if (FAILED(names->FindLocaleName(locale.c_str(), &index, &exists)) ||
      !exists) {
    return std::nullopt;
  }
  return ReadLocalizedName(names, index);
}

void AppendUniqueLocale(std::vector<std::wstring>& target,
                        const std::wstring& candidate) {
  if (candidate.empty()) return;
  for (const auto& existing : target) {
    if (_wcsicmp(existing.c_str(), candidate.c_str()) == 0) return;
  }
  target.push_back(candidate);
}

std::vector<std::wstring> BuildPreferredLocales(const std::string& requested) {
  std::string normalized = requested;
  std::replace(normalized.begin(), normalized.end(), '_', '-');
  const std::wstring requested_wide = WideFromUtf8(normalized);

  wchar_t user_locale_buf[LOCALE_NAME_MAX_LENGTH] = {};
  std::wstring user_locale;
  if (GetUserDefaultLocaleName(user_locale_buf, LOCALE_NAME_MAX_LENGTH) > 0) {
    user_locale = user_locale_buf;
  }

  std::vector<std::wstring> locales;
  if (StartsWithZh(requested_wide)) {
    AppendUniqueLocale(locales, requested_wide);
    if (StartsWithZh(user_locale)) {
      AppendUniqueLocale(locales, user_locale);
    }
    AppendUniqueLocale(locales, L"zh-cn");
    AppendUniqueLocale(locales, L"zh-hans");
    AppendUniqueLocale(locales, L"zh-tw");
    AppendUniqueLocale(locales, L"zh-hk");
  } else if (!requested_wide.empty()) {
    AppendUniqueLocale(locales, requested_wide);
    AppendUniqueLocale(locales, user_locale);
  } else {
    AppendUniqueLocale(locales, user_locale);
  }
  return locales;
}

bool EnumerateSystemFonts(const std::string& requested_locale,
                          bool refresh,
                          EncodableList& entries) {
  ComPtr<IDWriteFactory> factory;
  if (FAILED(DWriteCreateFactory(
          DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory),
          reinterpret_cast<IUnknown**>(factory.GetAddressOf()))) ||
      !factory) {
    return false;
  }

  ComPtr<IDWriteFontCollection> collection;
  if (FAILED(factory->GetSystemFontCollection(&collection,
                                              refresh ? TRUE : FALSE)) ||
      !collection) {
    return false;
  }

  const auto preferred_locales = BuildPreferredLocales(requested_locale);
  const UINT32 count = collection->GetFontFamilyCount();
  std::unordered_set<std::string> seen;
  seen.reserve(count);

  for (UINT32 i = 0; i < count; ++i) {
    ComPtr<IDWriteFontFamily> family;
    if (FAILED(collection->GetFontFamily(i, &family)) || !family) continue;

    ComPtr<IDWriteLocalizedStrings> names;
    if (FAILED(family->GetFamilyNames(&names)) || !names ||
        names->GetCount() == 0) {
      continue;
    }

    std::optional<std::string> raw_family = FindLocaleString(names.Get(), L"en-us");
    if (!raw_family.has_value()) {
      raw_family = ReadLocalizedName(names.Get(), 0);
    }
    if (!raw_family.has_value()) continue;

    const auto canonical = TrimAndValidateFamily(*raw_family);
    if (!canonical.has_value()) continue;

    const std::string key = AsciiLower(*canonical);
    if (!seen.insert(key).second) continue;

    std::string display_name = *canonical;
    for (const auto& locale : preferred_locales) {
      const auto localized = FindLocaleString(names.Get(), locale);
      if (!localized.has_value()) continue;
      const auto validated = TrimAndValidateFamily(*localized);
      if (validated.has_value()) {
        display_name = *validated;
        break;
      }
    }

    entries.emplace_back(EncodableMap{
        {EncodableValue("family"), EncodableValue(*canonical)},
        {EncodableValue("displayName"), EncodableValue(display_name)},
    });
  }

  return true;
}

}  // namespace

SystemFonts::SystemFonts(flutter::BinaryMessenger* messenger) {
  channel_ = std::make_unique<flutter::MethodChannel<EncodableValue>>(
      messenger, "morrow/system_fonts",
      &flutter::StandardMethodCodec::GetInstance());
  channel_->SetMethodCallHandler([](const auto& call, auto result) {
    if (call.method_name() != "listSystemFonts") {
      result->NotImplemented();
      return;
    }

    std::string locale;
    bool refresh = false;
    if (const auto* args = call.arguments()
                               ? std::get_if<EncodableMap>(call.arguments())
                               : nullptr) {
      if (const auto it = args->find(EncodableValue("locale"));
          it != args->end()) {
        if (const auto* value = std::get_if<std::string>(&it->second)) {
          locale = *value;
        }
      }
      if (const auto it = args->find(EncodableValue("refresh"));
          it != args->end()) {
        if (const auto* value = std::get_if<bool>(&it->second)) {
          refresh = *value;
        }
      }
    }

    EncodableList entries;
    if (EnumerateSystemFonts(locale, refresh, entries)) {
      result->Success(EncodableValue(std::move(entries)));
    } else {
      result->Error("system_fonts_unavailable",
                    "Unable to enumerate Windows system fonts.");
    }
  });
}

SystemFonts::~SystemFonts() {
  if (channel_) {
    channel_->SetMethodCallHandler(nullptr);
    channel_.reset();
  }
}
